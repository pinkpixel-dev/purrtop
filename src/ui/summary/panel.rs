//! The frame every Summary section sits in: a title row and a body.

use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

/// A bordered section with `title` on the left and `trailing` on the right.
/// Size it from the caller.
pub fn panel(title: &'static str, trailing: Option<AnyElement>, cx: &App) -> Div {
    let theme = cx.theme();
    v_flex()
        .p_3()
        .gap_2()
        .min_h_0()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.group_box)
        .child(
            h_flex()
                .flex_shrink_0()
                .gap_2()
                .justify_between()
                .items_center()
                .child(div().text_sm().font_semibold().child(title))
                .children(trailing),
        )
}

/// A colored dot and a label, for chart legends.
pub fn legend(label: &'static str, color: Hsla, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_1p5()
        .items_center()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(div().size_2().rounded_full().bg(color))
        .child(label)
}

/// Small muted text for footers and details.
pub fn caption(text: impl Into<SharedString>, cx: &App) -> Div {
    div().text_xs().text_color(cx.theme().muted_foreground).truncate().child(text.into())
}
