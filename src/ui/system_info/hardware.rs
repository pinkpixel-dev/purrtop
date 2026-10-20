//! The Hardware tab: the machine, then processor, memory, graphics, storage
//! and batteries from the latest snapshot.

use gpui_kit::*;

use super::{device_section, section, yes_no};
use crate::system::gpu::Vendor;
use crate::system::hardware::HardwareInfo;
use crate::system::{Snapshot, format};

pub fn render(info: &HardwareInfo, snapshot: &Snapshot, cx: &App) -> Vec<AnyElement> {
    let mut sections = vec![render_system(info, cx), processor(snapshot, cx), memory(snapshot, cx)];
    if !snapshot.gpus.is_empty() {
        sections.push(graphics(info, snapshot, cx));
    }
    if !snapshot.disks.is_empty() {
        sections.push(storage(snapshot, cx));
    }
    if !snapshot.batteries.is_empty() {
        sections.push(batteries(snapshot, cx));
    }
    sections
}

/// The maker, model and firmware. Needs no snapshot, so it shows right away.
pub fn render_system(info: &HardwareInfo, cx: &App) -> AnyElement {
    let boot_mode = if info.uefi { "UEFI" } else { "Legacy BIOS" };
    section(
        "System",
        vec![
            ("Manufacturer", info.manufacturer.clone()),
            ("Model", info.model.clone()),
            ("Family", info.family.clone()),
            ("Version", info.version.clone()),
            ("Type", info.chassis.map(str::to_string)),
            ("Motherboard", info.board.clone()),
            ("Firmware", info.firmware.clone()),
            ("Firmware date", info.firmware_date.clone()),
            ("Boot mode", Some(boot_mode.into())),
            ("Secure Boot", info.secure_boot.map(|on| if on { "On" } else { "Off" }.into())),
        ],
        cx,
    )
}

fn processor(snapshot: &Snapshot, cx: &App) -> AnyElement {
    let info = &snapshot.cpu_info;
    let core_types = info
        .hybrid_split()
        .map(|(performance, efficiency)| format!("{performance} P-core and {efficiency} E-core threads"));
    section(
        "Processor",
        vec![
            ("Name", (!info.model.is_empty()).then(|| info.model.clone())),
            ("Sockets", Some(info.sockets.to_string())),
            ("Physical cores", Some(info.physical_cores.to_string())),
            ("Logical processors", Some(info.logical.to_string())),
            ("Core types", core_types),
            ("Base speed", info.base_mhz.map(format::frequency)),
            ("Max speed", snapshot.clock.map(|clock| format::frequency(clock.max_mhz))),
            ("Virtualization", Some(info.virtualization.unwrap_or("Not available").into())),
            ("Virtual machine", Some(yes_no(info.virtual_machine))),
        ],
        cx,
    )
}

fn memory(snapshot: &Snapshot, cx: &App) -> AnyElement {
    let memory = &snapshot.memory;
    let swap = if memory.swap_total == 0 { "None".into() } else { format::bytes(memory.swap_total) };
    section("Memory", vec![("Usable", Some(format::bytes(memory.total))), ("Swap", Some(swap))], cx)
}

fn graphics(info: &HardwareInfo, snapshot: &Snapshot, cx: &App) -> AnyElement {
    let devices = snapshot
        .gpus
        .iter()
        .map(|gpu| {
            let version = (gpu.vendor == Vendor::Nvidia && gpu.driver.as_deref() == Some("nvidia"))
                .then(|| info.nvidia_driver.clone())
                .flatten();
            let driver = match (gpu.driver.clone(), version) {
                (Some(driver), Some(version)) => Some(format!("{driver} {version}")),
                (driver, _) => driver,
            };
            (
                gpu.name.clone(),
                vec![("Vendor", Some(gpu.vendor.label().into())), ("Driver", driver), ("PCI slot", Some(gpu.slot.clone()))],
            )
        })
        .collect();
    device_section("Graphics", devices, cx)
}

fn storage(snapshot: &Snapshot, cx: &App) -> AnyElement {
    let devices = snapshot
        .disks
        .iter()
        .enumerate()
        .map(|(ix, disk)| {
            let info = &disk.info;
            let mounts: Vec<&str> = disk.volumes.iter().map(|volume| volume.mount.as_str()).collect();
            let name = match &info.model {
                Some(model) => format!("Disk {ix} · {model}"),
                None => format!("Disk {ix}"),
            };
            (
                name,
                vec![
                    ("Device", Some(format!("/dev/{}", disk.name))),
                    ("Type", Some(info.kind.label().into())),
                    ("Capacity", (info.size > 0).then(|| format::bytes(info.size))),
                    ("Removable", Some(yes_no(info.removable))),
                    ("Mounted at", (!mounts.is_empty()).then(|| mounts.join(", "))),
                ],
            )
        })
        .collect();
    device_section("Storage", devices, cx)
}

fn batteries(snapshot: &Snapshot, cx: &App) -> AnyElement {
    let watt_hours = |wh: f32| format!("{wh:.1} Wh");
    let devices = snapshot
        .batteries
        .iter()
        .map(|battery| {
            (
                battery.name.clone(),
                vec![
                    ("Model", battery.model.clone()),
                    ("Manufacturer", battery.manufacturer.clone()),
                    ("Chemistry", battery.technology.clone()),
                    ("Design capacity", battery.design_wh.map(watt_hours)),
                    ("Full charge", battery.full_wh.map(watt_hours)),
                    ("Health", battery.health.map(format::percent)),
                    ("Charge cycles", battery.cycles.map(|cycles| cycles.to_string())),
                ],
            )
        })
        .collect();
    device_section("Batteries", devices, cx)
}
