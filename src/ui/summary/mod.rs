//! The page purrtop opens on: the whole machine at a glance.

mod cards;
mod cpu;
mod gauges;
mod memory;
mod panel;
mod top;

use gpui_kit::component::StyledExt as _;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::charts::Palette;
use crate::system::monitor::Monitor;

/// Below this width the top processes list moves under the CPU chart.
const WIDE_LAYOUT: f32 = 1180.;
/// Vertical space taken by everything but the top row: title bar, page
/// heading, memory panel, cards, status bar, padding and gaps.
const RESERVED_HEIGHT: f32 = 480.;
const MIN_TOP_HEIGHT: f32 = 300.;
const GAUGES_WIDTH: f32 = 176.;
const TOP_WIDTH: f32 = 380.;
const NARROW_TOP_HEIGHT: f32 = 300.;

pub fn render(monitor: &Monitor, window: &Window, cx: &App) -> AnyElement {
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
    let palette = Palette::new(cx);
    let viewport = window.viewport_size();
    let wide = f32::from(viewport.width) >= WIDE_LAYOUT;
    let top_height = (f32::from(viewport.height) - RESERVED_HEIGHT).max(MIN_TOP_HEIGHT);

    let gauges = gauges::render(snapshot, &palette, cx).w(px(GAUGES_WIDTH)).flex_shrink_0();
    let cpu = cpu::render(monitor, snapshot, &palette, cx).flex_1().min_w_0();
    let top = top::render(snapshot, &palette, cx);

    let overview = h_flex().h(px(top_height)).gap_3().items_stretch().child(gauges).child(cpu);
    let overview = if wide {
        overview.child(top.w(px(TOP_WIDTH)).flex_shrink_0()).into_any_element()
    } else {
        v_flex().gap_3().child(overview).child(top.h(px(NARROW_TOP_HEIGHT))).into_any_element()
    };

    v_flex()
        .id("summary")
        .size_full()
        .p_4()
        .gap_3()
        .overflow_y_scrollbar()
        .child(div().text_lg().font_semibold().child("Summary"))
        .child(overview)
        .child(memory::render(monitor, snapshot, &palette, cx))
        .child(cards::render(monitor, snapshot, &palette, cx))
        .into_any_element()
}
