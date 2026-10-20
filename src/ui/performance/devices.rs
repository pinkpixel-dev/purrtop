//! The device list: one row per device with its current reading and a sparkline.

use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::system::history::History;
use crate::system::monitor::Monitor;
use crate::system::{Snapshot, format};
use crate::ui::charts::{Grid, Palette, Series, line_chart};

const LIST_WIDTH: f32 = 272.;
const COMPACT_WIDTH: f32 = 184.;
const ROW_HEIGHT: f32 = 60.;
const SPARKLINE_WIDTH: f32 = 84.;
const SPARKLINE_HEIGHT: f32 = 44.;

pub type OnSelect = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Device {
    #[default]
    Cpu,
    Memory,
}

impl Device {
    pub const ALL: [Device; 2] = [Device::Cpu, Device::Memory];

    fn label(self) -> &'static str {
        match self {
            Device::Cpu => "CPU",
            Device::Memory => "Memory",
        }
    }

    fn icon(self) -> CatalogIcon {
        match self {
            Device::Cpu => CatalogIcon::Cpu,
            Device::Memory => CatalogIcon::MemoryStick,
        }
    }
}

/// What a row shows: the headline reading, a detail line, and its history.
struct Reading<'a> {
    value: String,
    detail: String,
    color: Hsla,
    history: &'a History,
}

/// How wide the list is, so the CPU grid can work out the space left over.
pub fn width(compact: bool) -> f32 {
    if compact { COMPACT_WIDTH } else { LIST_WIDTH }
}

fn reading<'a>(device: Device, monitor: &'a Monitor, snapshot: &Snapshot, palette: &Palette) -> Reading<'a> {
    match device {
        Device::Cpu => Reading {
            value: format::percent(snapshot.cpu),
            detail: snapshot.clock.map_or_else(
                || format!("{} logical processors", snapshot.cores.len()),
                |clock| format::frequency(clock.current_mhz),
            ),
            color: palette.cpu,
            history: &monitor.cpu,
        },
        Device::Memory => {
            let memory = snapshot.memory;
            Reading {
                value: format::percent(memory.used_fraction()),
                detail: format!("{} / {}", format::bytes(memory.used()), format::bytes(memory.total)),
                color: palette.memory,
                history: &monitor.memory,
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn render(
    selected: Device,
    focused: bool,
    compact: bool,
    monitor: &Monitor,
    snapshot: &Snapshot,
    palette: &Palette,
    cx: &App,
    on_select: impl Fn(Device) -> OnSelect,
) -> impl IntoElement {
    let rows = Device::ALL.into_iter().map(|device| {
        let reading = reading(device, monitor, snapshot, palette);
        row(device, reading, device == selected, focused, compact, palette, cx).on_click(on_select(device))
    });
    v_flex()
        .w(px(width(compact)))
        .flex_shrink_0()
        .gap_1()
        .children(rows)
}

fn row(
    device: Device,
    reading: Reading,
    selected: bool,
    focused: bool,
    compact: bool,
    palette: &Palette,
    cx: &App,
) -> Stateful<Div> {
    let theme = cx.theme();
    let sparkline = div()
        .w(px(SPARKLINE_WIDTH))
        .h(px(SPARKLINE_HEIGHT))
        .flex_shrink_0()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .overflow_hidden()
        .child(line_chart(
            vec![Series::new(reading.history, 1., reading.color).area()],
            Some(Grid { rows: 3, columns: 4, color: palette.grid }),
        ));

    h_flex()
        .id(SharedString::from(format!("device-{}", device.label())))
        .h(px(ROW_HEIGHT))
        .px_2()
        .gap_3()
        .items_center()
        .rounded(theme.radius)
        .border_1()
        .cursor_pointer()
        .border_color(if selected && focused { theme.ring } else { transparent_black() })
        .when(selected, |this| this.bg(theme.list_active))
        .when(!selected, |this| this.hover(|style| style.bg(theme.list_hover)))
        .child(Icon::new(device.icon()).small().text_color(if selected { theme.primary } else { theme.muted_foreground }))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(div().text_sm().font_semibold().truncate().child(device.label()))
                .child(div().text_xs().text_color(reading.color).child(reading.value))
                .child(div().text_xs().text_color(theme.muted_foreground).truncate().child(reading.detail)),
        )
        .when(!compact, |this| this.child(sparkline))
}
