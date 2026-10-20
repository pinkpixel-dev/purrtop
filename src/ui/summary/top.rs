//! The busiest processes right now.

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::panel::{caption, panel};
use crate::system::{Snapshot, format};
use crate::ui::charts::Palette;

const ROW_HEIGHT: f32 = 22.;
const PID_WIDTH: f32 = 64.;
const CPU_WIDTH: f32 = 60.;
const MEMORY_WIDTH: f32 = 76.;
/// Processes using at least this much CPU get their usage highlighted.
const HIGHLIGHT_CPU: f32 = 0.01;

pub fn render(snapshot: &Snapshot, palette: &Palette, cx: &App) -> Div {
    let theme = cx.theme();
    let header = h_flex()
        .flex_shrink_0()
        .px_1()
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(div().w(px(PID_WIDTH)).child("PID"))
        .child(div().flex_1().child("Name"))
        .child(div().w(px(CPU_WIDTH)).text_right().child("CPU"))
        .child(div().w(px(MEMORY_WIDTH)).text_right().child("Memory"));

    let rows = snapshot.top.iter().map(|process| {
        let busy = process.cpu >= HIGHLIGHT_CPU;
        h_flex()
            .h(px(ROW_HEIGHT))
            .flex_shrink_0()
            .px_1()
            .rounded(theme.radius)
            .text_xs()
            .when(busy, |this| this.bg(palette.cpu.opacity(0.1)))
            .child(div().w(px(PID_WIDTH)).text_color(theme.muted_foreground).child(process.pid.to_string()))
            .child(div().flex_1().min_w_0().truncate().child(process.name.clone()))
            .child(
                div()
                    .w(px(CPU_WIDTH))
                    .text_right()
                    .text_color(if busy { palette.cpu } else { theme.muted_foreground })
                    .child(format::percent(process.cpu)),
            )
            .child(div().w(px(MEMORY_WIDTH)).text_right().child(format::bytes(process.memory)))
    });

    let trailing = caption(format!("{} running", snapshot.processes), cx).into_any_element();
    panel("Top processes", Some(trailing), cx).child(header).child(
        v_flex().id("top-processes").flex_1().min_h_0().gap_px().overflow_y_scrollbar().children(rows),
    )
}
