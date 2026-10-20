//! The device list: one row per device with its current reading and a sparkline.

use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::parts::{NETWORK_FLOOR, shared_scale};
use crate::system::battery::{Battery, BatteryState};
use crate::system::gpu::GpuReading;
use crate::system::io::{Disk, Interface};
use crate::system::monitor::Monitor;
use crate::system::{Snapshot, format};
use crate::ui::charts::{Grid, Palette, Series, line_chart};

const LIST_WIDTH: f32 = 272.;
const COMPACT_WIDTH: f32 = 184.;
const ROW_HEIGHT: f32 = 60.;
const SPARKLINE_WIDTH: f32 = 84.;
const SPARKLINE_HEIGHT: f32 = 44.;
/// The top of every temperature chart, in °C.
pub const THERMAL_SCALE: f32 = 110.;

pub type OnSelect = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// Disks, interfaces and batteries are kept by kernel name and GPUs by PCI
/// slot, so the selection survives devices coming and going.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Device {
    #[default]
    Cpu,
    Memory,
    Battery(String),
    Thermals,
    Disk(String),
    Network(String),
    Gpu(String),
}

impl Device {
    /// Every device in list order: CPU, memory, batteries, thermals, disks,
    /// interfaces, then GPUs.
    pub fn list(snapshot: &Snapshot) -> Vec<Device> {
        let mut devices = vec![Device::Cpu, Device::Memory];
        devices.extend(snapshot.batteries.iter().map(|battery| Device::Battery(battery.name.clone())));
        if !snapshot.thermals.is_empty() {
            devices.push(Device::Thermals);
        }
        devices.extend(snapshot.disks.iter().map(|disk| Device::Disk(disk.name.clone())));
        devices.extend(snapshot.interfaces.iter().map(|interface| Device::Network(interface.name.clone())));
        devices.extend(snapshot.gpus.iter().map(|gpu| Device::Gpu(gpu.slot.clone())));
        devices
    }

    fn key(&self) -> String {
        match self {
            Device::Cpu => "cpu".into(),
            Device::Memory => "memory".into(),
            Device::Battery(name) => format!("battery-{name}"),
            Device::Thermals => "thermals".into(),
            Device::Disk(name) => format!("disk-{name}"),
            Device::Network(name) => format!("net-{name}"),
            Device::Gpu(slot) => format!("gpu-{slot}"),
        }
    }
}

/// `Disk 0`, `Disk 1`… in kernel name order.
pub fn disk_label(disk: &Disk, snapshot: &Snapshot) -> String {
    let ix = snapshot.disks.iter().position(|other| other.name == disk.name).unwrap_or(0);
    format!("Disk {ix}")
}

/// `Wi-Fi` or `Ethernet`, numbered from the second one of a kind.
pub fn interface_label(interface: &Interface, snapshot: &Snapshot) -> String {
    let kind = if interface.info.wireless { "Wi-Fi" } else { "Ethernet" };
    let same_kind: Vec<&Interface> = snapshot.interfaces.iter().filter(|other| other.info.wireless == interface.info.wireless).collect();
    match same_kind.iter().position(|other| other.name == interface.name) {
        Some(ix) if ix > 0 => format!("{kind} {}", ix + 1),
        _ => kind.into(),
    }
}

/// `Battery`, numbered from the second one.
pub fn battery_label(battery: &Battery, snapshot: &Snapshot) -> String {
    match snapshot.batteries.iter().position(|other| other.name == battery.name) {
        Some(ix) if ix > 0 => format!("Battery {}", ix + 1),
        _ => "Battery".into(),
    }
}

/// The GPU's headline: usage when the driver gives it, otherwise its clock,
/// otherwise just whether it's awake.
pub fn gpu_headline(gpu: &GpuReading) -> String {
    match (gpu.busy, gpu.clock_mhz, gpu.awake) {
        (Some(busy), _, _) => format::percent(busy),
        (None, Some(mhz), _) => format::frequency(mhz),
        (None, None, true) => "Awake".into(),
        (None, None, false) => "Sleeping".into(),
    }
}

/// What a row shows: its name, the headline reading, a detail line, and a sparkline.
struct Reading {
    label: String,
    icon: CatalogIcon,
    value: String,
    detail: String,
    color: Hsla,
    series: Vec<Series>,
}

fn reading(device: &Device, monitor: &Monitor, snapshot: &Snapshot, palette: &Palette) -> Option<Reading> {
    Some(match device {
        Device::Cpu => Reading {
            label: "CPU".into(),
            icon: CatalogIcon::Cpu,
            value: format::percent(snapshot.cpu),
            detail: snapshot.clock.map_or_else(
                || format!("{} logical processors", snapshot.cores.len()),
                |clock| format::frequency(clock.current_mhz),
            ),
            color: palette.cpu,
            series: vec![Series::new(&monitor.cpu, 1., palette.cpu).area()],
        },
        Device::Memory => {
            let memory = snapshot.memory;
            Reading {
                label: "Memory".into(),
                icon: CatalogIcon::MemoryStick,
                value: format::percent(memory.used_fraction()),
                detail: format!("{} / {}", format::bytes(memory.used()), format::bytes(memory.total)),
                color: palette.memory,
                series: vec![Series::new(&monitor.memory, 1., palette.memory).area()],
            }
        }
        Device::Disk(name) => {
            let disk = snapshot.disks.iter().find(|disk| disk.name == *name)?;
            let history = monitor.disks.get(name)?;
            let mounts: Vec<&str> = disk.volumes.iter().map(|volume| volume.mount.as_str()).collect();
            Reading {
                label: disk_label(disk, snapshot),
                icon: if disk.info.removable { CatalogIcon::Usb } else { CatalogIcon::HardDrive },
                value: format::percent(disk.rates.busy),
                detail: if mounts.is_empty() { disk.info.kind.label().into() } else { format!("{} · {}", disk.info.kind.label(), mounts.join(", ")) },
                color: palette.read,
                series: vec![Series::new(&history.busy, 1., palette.read).area()],
            }
        }
        Device::Network(name) => {
            let interface = snapshot.interfaces.iter().find(|interface| interface.name == *name)?;
            let history = monitor.interfaces.get(name)?;
            let scale = shared_scale([&history.received, &history.sent], NETWORK_FLOOR);
            Reading {
                label: interface_label(interface, snapshot),
                icon: if interface.info.wireless { CatalogIcon::Wifi } else { CatalogIcon::EthernetPort },
                value: format!("↓ {}  ↑ {}", format::rate(interface.rates.received), format::rate(interface.rates.sent)),
                detail: if interface.info.connected { name.clone() } else { format!("{name} · Not connected") },
                color: palette.received,
                series: vec![
                    Series::new(&history.sent, scale, palette.sent),
                    Series::new(&history.received, scale, palette.received).area(),
                ],
            }
        }
        Device::Battery(name) => {
            let battery = snapshot.batteries.iter().find(|battery| battery.name == *name)?;
            let history = monitor.batteries.get(name)?;
            let mut detail = battery.state.label().to_string();
            if let Some(left) = battery.time_left {
                detail.push_str(&format!(" · {} left", format::duration(left)));
            }
            Reading {
                label: battery_label(battery, snapshot),
                icon: if battery.state == BatteryState::Charging { CatalogIcon::BatteryCharging } else { CatalogIcon::Battery },
                value: format::percent(battery.charge),
                detail,
                color: palette.battery,
                series: vec![Series::new(&history.charge, 1., palette.battery).area()],
            }
        }
        Device::Thermals => {
            let hottest = snapshot.thermals.iter().max_by(|a, b| a.celsius.total_cmp(&b.celsius))?;
            let history = monitor.thermals.get(&hottest.key)?;
            Reading {
                label: "Thermals".into(),
                icon: CatalogIcon::Thermometer,
                value: format::celsius(hottest.celsius),
                detail: format!("Hottest: {}", hottest.label),
                color: palette.temperature,
                series: vec![Series::new(history, THERMAL_SCALE, palette.temperature).area()],
            }
        }
        Device::Gpu(slot) => {
            let gpu = snapshot.gpus.iter().find(|gpu| gpu.slot == *slot)?;
            let history = monitor.gpus.get(slot)?;
            let series = match (gpu.busy.is_some(), gpu.max_clock_mhz) {
                (true, _) => vec![Series::new(&history.busy, 1., palette.gpu).area()],
                (false, Some(max)) => vec![Series::new(&history.clock, max, palette.gpu).area()],
                (false, None) => Vec::new(),
            };
            let mut detail = gpu.vendor.label().to_string();
            if let Some(driver) = &gpu.driver {
                detail.push_str(&format!(" · {driver}"));
            }
            Reading {
                label: gpu.name.clone(),
                icon: CatalogIcon::Gpu,
                value: gpu_headline(gpu),
                detail,
                color: palette.gpu,
                series,
            }
        }
    })
}

/// How wide the list is, so the CPU grid can work out the space left over.
pub fn width(compact: bool) -> f32 {
    if compact { COMPACT_WIDTH } else { LIST_WIDTH }
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    devices: &[Device],
    selected: &Device,
    focused: bool,
    compact: bool,
    monitor: &Monitor,
    snapshot: &Snapshot,
    palette: &Palette,
    scroll: &ScrollHandle,
    cx: &App,
    on_select: impl Fn(Device) -> OnSelect,
) -> impl IntoElement {
    let rows = devices.iter().filter_map(|device| {
        let reading = reading(device, monitor, snapshot, palette)?;
        Some(row(device, reading, device == selected, focused, compact, palette, cx).on_click(on_select(device.clone())))
    });
    v_flex()
        .id("performance-devices")
        .w(px(width(compact)))
        .flex_shrink_0()
        .h_full()
        .gap_1()
        .overflow_y_scroll()
        .track_scroll(scroll)
        .children(rows)
}

fn row(device: &Device, reading: Reading, selected: bool, focused: bool, compact: bool, palette: &Palette, cx: &App) -> Stateful<Div> {
    let theme = cx.theme();
    let sparkline = div()
        .w(px(SPARKLINE_WIDTH))
        .h(px(SPARKLINE_HEIGHT))
        .flex_shrink_0()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .child(line_chart(reading.series, Some(Grid { rows: 3, columns: 4, color: palette.grid })));

    h_flex()
        .id(SharedString::from(format!("device-{}", device.key())))
        .h(px(ROW_HEIGHT))
        .flex_shrink_0()
        .px_2()
        .gap_3()
        .items_center()
        .rounded(theme.radius)
        .border_1()
        .cursor_pointer()
        .border_color(if selected && focused { theme.ring } else { transparent_black() })
        .when(selected, |this| this.bg(theme.list_active))
        .when(!selected, |this| this.hover(|style| style.bg(theme.list_hover)))
        .child(Icon::new(reading.icon).small().text_color(if selected { theme.primary } else { theme.muted_foreground }))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(div().text_sm().font_semibold().truncate().child(reading.label))
                .child(div().text_xs().text_color(reading.color).truncate().child(reading.value))
                .child(div().text_xs().text_color(theme.muted_foreground).truncate().child(reading.detail)),
        )
        .when(!compact, |this| this.child(sparkline))
}

#[cfg(test)]
mod tests {
    use super::{Device, disk_label, interface_label};
    use crate::system::Snapshot;
    use crate::system::io::{Disk, Interface};
    use crate::system::net_info::InterfaceInfo;

    fn interface(name: &str, wireless: bool) -> Interface {
        Interface { name: name.into(), info: InterfaceInfo { wireless, ..InterfaceInfo::default() }, ..Interface::default() }
    }

    #[test]
    fn lists_and_labels_devices() {
        let snapshot = Snapshot {
            disks: vec![Disk { name: "nvme0n1".into(), ..Disk::default() }, Disk { name: "nvme1n1".into(), ..Disk::default() }],
            interfaces: vec![interface("wlan0", true), interface("enp1s0", false), interface("enp2s0", false)],
            ..Snapshot::default()
        };
        let devices = Device::list(&snapshot);
        assert_eq!(devices.len(), 7);
        assert_eq!(devices[2], Device::Disk("nvme0n1".into()));
        assert_eq!(devices[6], Device::Network("enp2s0".into()));
        assert_eq!(disk_label(&snapshot.disks[1], &snapshot), "Disk 1");
        let labels: Vec<String> = snapshot.interfaces.iter().map(|interface| interface_label(interface, &snapshot)).collect();
        assert_eq!(labels, ["Wi-Fi", "Ethernet", "Ethernet 2"]);
    }
}
