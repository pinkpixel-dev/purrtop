//! Memory use over time.

use gpui_kit::component::StyledExt as _;
use gpui_kit::component::h_flex;
use gpui_kit::*;

use super::panel::{caption, panel};
use crate::system::monitor::Monitor;
use crate::system::{Snapshot, format};
use crate::ui::charts::{Grid, Palette, Series, line_chart};

const CHART_HEIGHT: f32 = 88.;

pub fn render(monitor: &Monitor, snapshot: &Snapshot, palette: &Palette, cx: &App) -> Div {
    let memory = snapshot.memory;
    let used = div()
        .text_sm()
        .font_semibold()
        .text_color(palette.memory)
        .child(format!("{} / {}", format::bytes(memory.used()), format::bytes(memory.total)));
    let swap = if memory.swap_total == 0 {
        "No swap".to_string()
    } else {
        format!("Swap {} / {}", format::bytes(memory.swap_used()), format::bytes(memory.swap_total))
    };

    panel("Memory", Some(used.into_any_element()), cx)
        .flex_shrink_0()
        .child(div().h(px(CHART_HEIGHT)).child(line_chart(
            vec![Series::new(&monitor.memory, 1., palette.memory).area()],
            Some(Grid { rows: 4, columns: 24, color: palette.grid }),
        )))
        .child(
            h_flex()
                .justify_between()
                .gap_3()
                .child(caption(
                    format!(
                        "{} used · {} available · {} cached",
                        format::percent(memory.used_fraction()),
                        format::bytes(memory.available),
                        format::bytes(memory.cached)
                    ),
                    cx,
                ))
                .child(caption(swap, cx)),
        )
}
