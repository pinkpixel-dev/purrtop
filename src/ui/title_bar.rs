use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{TitleBar, h_flex};
use gpui_kit::*;

use super::Purrtop;
use crate::{app_info, assets};

/// Title bar height; gpui-kit's default is 34px.
const TITLE_BAR_HEIGHT: f32 = 40.;
const LOGO_SIZE: f32 = 24.;

impl Purrtop {
    pub(super) fn render_title_bar(&self, _: &mut Context<Self>) -> impl IntoElement {
        TitleBar::new().h(px(TITLE_BAR_HEIGHT)).child(
            h_flex()
                .gap_3()
                .items_center()
                .child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .flex_shrink_0()
                        .child(img(assets::LOGO).size(px(LOGO_SIZE)).flex_shrink_0())
                        .child(div().text_sm().font_semibold().child(app_info::NAME)),
                )
                .child(self.menu_bar.clone()),
        )
    }
}
