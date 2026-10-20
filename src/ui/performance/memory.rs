//! The Memory detail: use and swap over time, what RAM is holding, and stats.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::parts::{header, stats};
use crate::system::monitor::Monitor;
use crate::system::{Snapshot, format};
use crate::ui::charts::{Grid, Palette, Series, line_chart};
use crate::ui::summary::panel::legend;

/// Vertical space taken by everything but the chart: title and status bars,
/// page heading, the header, legend, composition, stats, padding and gaps.
const RESERVED_HEIGHT: f32 = 400.;
const MIN_CHART_HEIGHT: f32 = 180.;
const BAR_HEIGHT: f32 = 12.;
/// How strongly cached memory is tinted in the composition bar.
const CACHED_OPACITY: f32 = 0.4;

pub fn render(monitor: &Monitor, snapshot: &Snapshot, palette: &Palette, viewport: Size<Pixels>, cx: &App) -> Div {
    let memory = snapshot.memory;
    let has_swap = memory.swap_total > 0;
    let chart_height = (f32::from(viewport.height) - RESERVED_HEIGHT).max(MIN_CHART_HEIGHT);

    let mut series = Vec::new();
    if has_swap {
        series.push(Series::new(&monitor.swap, 1., palette.swap));
    }
    series.push(Series::new(&monitor.memory, 1., palette.memory).area());

    let parts = [
        ("In use", memory.used(), palette.memory),
        ("Cached", memory.cached, palette.memory.opacity(CACHED_OPACITY)),
        ("Free", memory.free(), cx.theme().muted),
    ];

    v_flex()
        .gap_3()
        .child(header(
            "Memory",
            format!("{} total", format::bytes(memory.total)),
            memory.used_fraction(),
            format::percent(memory.used_fraction()),
            palette.memory,
            cx,
        ))
        .child(
            h_flex()
                .gap_4()
                .child(legend("In use", palette.memory, cx))
                .when(has_swap, |this| this.child(legend("Swap", palette.swap, cx))),
        )
        .child(
            div()
                .h(px(chart_height))
                .flex_shrink_0()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .overflow_hidden()
                .child(line_chart(series, Some(Grid { rows: 10, columns: 24, color: palette.grid }))),
        )
        .child(composition(&parts, memory.total, cx))
        .child(stats(
            vec![
                ("In use", format::bytes(memory.used())),
                ("Available", format::bytes(memory.available)),
                ("Cached", format::bytes(memory.cached)),
                ("Committed", format!("{} / {}", format::bytes(memory.committed), format::bytes(memory.commit_limit))),
                (
                    "Swap",
                    if has_swap {
                        format!("{} / {}", format::bytes(memory.swap_used()), format::bytes(memory.swap_total))
                    } else {
                        "None".into()
                    },
                ),
            ],
            cx,
        ))
}

/// One bar split into what RAM holds, with a labeled key under it so the
/// split never depends on color alone.
fn composition(parts: &[(&'static str, u64, Hsla)], total: u64, cx: &App) -> Div {
    let theme = cx.theme();
    let bar = h_flex()
        .h(px(BAR_HEIGHT))
        .w_full()
        .gap_0p5()
        .rounded(theme.radius)
        .overflow_hidden()
        .children(parts.iter().filter(|(_, bytes, _)| *bytes > 0).map(|&(_, bytes, color)| {
            div().h_full().w(relative(share(bytes, total))).bg(color)
        }));
    let key = h_flex().flex_wrap().gap_x_4().gap_y_1().children(parts.iter().map(|&(label, bytes, color)| {
        h_flex()
            .gap_1p5()
            .items_center()
            .text_xs()
            .child(div().size_2().rounded_full().bg(color))
            .child(div().text_color(theme.muted_foreground).child(label))
            .child(format::bytes(bytes))
    }));
    v_flex().flex_shrink_0().gap_2().child(bar).child(key)
}

fn share(part: u64, total: u64) -> f32 {
    if total == 0 { 0. } else { part as f32 / total as f32 }
}
