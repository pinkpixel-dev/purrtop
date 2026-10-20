//! The page purrtop opens on: the whole machine at a glance.

use gpui_kit::component::StyledExt as _;
use gpui_kit::component::progress::Progress;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use crate::system::format;
use crate::system::monitor::Monitor;

const TILE_MIN_WIDTH: f32 = 240.;

pub fn render(monitor: &Monitor, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let Some(snapshot) = monitor.latest() else {
        return div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(monitor.error.clone().unwrap_or_else(|| "Taking the first reading…".into()))
            .into_any_element();
    };
    let memory = snapshot.memory;
    let [load1, load5, load15] = snapshot.load;

    let tiles = [
        tile(
            "cpu",
            "CPU",
            format::percent(snapshot.cpu),
            format!("Load {load1:.2} · {load5:.2} · {load15:.2}"),
            Some(snapshot.cpu),
            theme.chart_1,
            cx,
        ),
        tile(
            "memory",
            "Memory",
            format!("{} / {}", format::bytes(memory.used()), format::bytes(memory.total)),
            format!("{} used · {} cached", format::percent(memory.used_fraction()), format::bytes(memory.cached)),
            Some(memory.used_fraction()),
            theme.chart_2,
            cx,
        ),
        tile(
            "swap",
            "Swap",
            format!("{} / {}", format::bytes(memory.swap_used()), format::bytes(memory.swap_total)),
            if memory.swap_total == 0 { "No swap configured".into() } else { "In use".into() },
            (memory.swap_total > 0).then(|| memory.swap_used() as f32 / memory.swap_total as f32),
            theme.chart_5,
            cx,
        ),
        tile(
            "processes",
            "Processes",
            snapshot.processes.to_string(),
            format!("{} logical processors", snapshot.cores.len()),
            None,
            theme.chart_3,
            cx,
        ),
        tile("uptime", "Uptime", format::uptime(snapshot.uptime), "Since last boot".into(), None, theme.chart_4, cx),
    ];

    v_flex()
        .id("summary")
        .size_full()
        .p_5()
        .gap_4()
        .overflow_y_scrollbar()
        .child(div().text_xl().font_semibold().child("Summary"))
        .child(h_flex().flex_wrap().items_stretch().gap_3().children(tiles))
        .into_any_element()
}

/// A labeled reading with an optional bar. `fraction` is 0 to 1.
fn tile(
    id: &'static str,
    label: &'static str,
    value: String,
    detail: String,
    fraction: Option<f32>,
    accent: Hsla,
    cx: &App,
) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .flex_1()
        .min_w(px(TILE_MIN_WIDTH))
        .p_4()
        .gap_2()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.group_box)
        .child(
            h_flex()
                .gap_2()
                .items_center()
                .child(div().size_2().rounded_full().bg(accent))
                .child(div().text_xs().font_medium().text_color(theme.muted_foreground).child(label)),
        )
        .child(div().text_2xl().font_semibold().child(value))
        .children(fraction.map(|fraction| {
            Progress::new(id).value(fraction * 100.).color(accent).accessibility_label(label)
        }))
        .child(div().text_xs().text_color(theme.muted_foreground).truncate().child(detail))
        .into_any_element()
}
