use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{ActiveTheme as _, Icon, TitleBar, h_flex};
use gpui_kit::*;

use super::Purrtop;
use crate::app_info;

/// Title bar height; gpui-kit's default is 34px.
const TITLE_BAR_HEIGHT: f32 = 40.;

impl Purrtop {
    pub(super) fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        TitleBar::new().h(px(TITLE_BAR_HEIGHT)).child(
            h_flex()
                .gap_3()
                .items_center()
                .child(
                    h_flex()
                        .gap_1p5()
                        .items_center()
                        .flex_shrink_0()
                        .child(Icon::new(CatalogIcon::Cat).text_color(theme.primary))
                        .child(div().text_sm().font_semibold().child(app_info::NAME)),
                )
                .child(self.menu_bar.clone()),
        )
    }
}
