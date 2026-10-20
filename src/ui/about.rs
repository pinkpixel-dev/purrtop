use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;

use crate::app_info;

const DIALOG_WIDTH: f32 = 360.;

pub fn open(window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    window.open_dialog(cx, |dialog, _, cx| {
        let theme = cx.theme();
        dialog
            .w(px(DIALOG_WIDTH))
            .child(
                v_flex()
                    .items_center()
                    .gap_2()
                    .py_2()
                    .child(Icon::new(CatalogIcon::Cat).large().text_color(theme.primary))
                    .child(
                        h_flex()
                            .gap_2()
                            .items_baseline()
                            .child(div().text_xl().font_semibold().child(app_info::NAME))
                            .child(div().text_sm().text_color(theme.muted_foreground).child(app_info::VERSION)),
                    )
                    .child(div().text_sm().text_color(theme.muted_foreground).child(app_info::TAGLINE))
                    .child(div().pt_2().text_xs().text_color(theme.muted_foreground).child("Made by Pink Pixel")),
            )
            .footer(
                h_flex()
                    .w_full()
                    .justify_between()
                    .child(
                        Button::new("about-website")
                            .ghost()
                            .label("pinkpixel.dev")
                            .on_click(|_, _, cx| cx.open_url(app_info::WEBSITE)),
                    )
                    .child(
                        Button::new("about-close")
                            .outline()
                            .label("Close")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
    });
}
