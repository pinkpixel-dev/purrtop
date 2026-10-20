//! Pieces every device detail shares: the title and meter, big stats, and a
//! two-column list of facts.

use gpui_kit::component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::*;

use crate::ui::charts::meter;

const METER_HEIGHT: f32 = 14.;
const FACT_LABEL_WIDTH: f32 = 140.;
const FACT_COLUMN_MIN_WIDTH: f32 = 280.;

/// The device name with a muted subtitle, then a meter lit to `fraction`
/// with the reading beside it.
pub fn header(title: &'static str, subtitle: String, fraction: f32, value: String, color: Hsla, cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .flex_shrink_0()
        .gap_2()
        .child(
            h_flex()
                .gap_3()
                .items_baseline()
                .justify_between()
                .child(div().text_xl().font_semibold().child(title))
                .child(div().min_w_0().text_sm().text_color(theme.muted_foreground).truncate().child(subtitle)),
        )
        .child(
            h_flex()
                .gap_3()
                .items_center()
                .child(div().flex_1().h(px(METER_HEIGHT)).child(meter(fraction, color)))
                .child(div().flex_shrink_0().text_lg().font_semibold().text_color(color).child(value)),
        )
}

/// A row of headline numbers, wrapping on narrow windows.
pub fn stats(items: Vec<(&'static str, String)>, cx: &App) -> Div {
    let theme = cx.theme();
    h_flex().flex_shrink_0().flex_wrap().gap_x_8().gap_y_3().children(items.into_iter().map(|(label, value)| {
        v_flex()
            .gap_0p5()
            .child(div().text_xs().text_color(theme.muted_foreground).child(label))
            .child(div().text_base().font_semibold().child(value))
    }))
}

/// Label and value pairs in two columns that stack on narrow windows.
pub fn facts(items: Vec<(&'static str, String)>, cx: &App) -> Div {
    let theme = cx.theme();
    let half = items.len().div_ceil(2);
    let mut items = items.into_iter();
    let column = |items: Vec<(&'static str, String)>| {
        v_flex().flex_1().min_w(px(FACT_COLUMN_MIN_WIDTH)).gap_1().children(items.into_iter().map(|(label, value)| {
            h_flex()
                .gap_2()
                .text_xs()
                .child(div().w(px(FACT_LABEL_WIDTH)).flex_shrink_0().text_color(theme.muted_foreground).child(label))
                .child(div().min_w_0().truncate().child(value))
        }))
    };
    let left: Vec<_> = items.by_ref().take(half).collect();
    let right: Vec<_> = items.collect();
    h_flex().flex_shrink_0().flex_wrap().gap_x_8().gap_y_1().items_start().child(column(left)).child(column(right))
}
