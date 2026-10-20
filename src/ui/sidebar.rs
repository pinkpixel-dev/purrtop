use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{Page, Purrtop};

const SIDEBAR_WIDTH: f32 = 208.;
const ROW_HEIGHT: f32 = 32.;

impl Purrtop {
    pub(super) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<AnyElement> = Page::ALL.into_iter().map(|page| self.sidebar_row(page, cx)).collect();
        v_flex()
            .id("sidebar")
            .w(px(SIDEBAR_WIDTH))
            .flex_shrink_0()
            .h_full()
            .px_2()
            .py_3()
            .gap_0p5()
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .bg(cx.theme().sidebar)
            .overflow_y_scrollbar()
            .children(rows)
    }

    fn sidebar_row(&self, page: Page, cx: &mut Context<Self>) -> AnyElement {
        let selected = self.page == page;
        let theme = cx.theme();

        h_flex()
            .id(SharedString::from(format!("page-{}", page.label())))
            .h(px(ROW_HEIGHT))
            .flex_shrink_0()
            .px_2()
            .gap_2()
            .items_center()
            .rounded(theme.radius)
            .text_sm()
            .cursor_pointer()
            .when(selected, |this| this.bg(theme.sidebar_accent).text_color(theme.sidebar_accent_foreground))
            .when(!selected, |this| this.text_color(theme.sidebar_foreground).hover(|style| style.bg(theme.list_hover)))
            .child(Icon::new(page.icon()).small().text_color(if selected { theme.primary } else { theme.muted_foreground }))
            .child(div().flex_1().min_w_0().truncate().child(page.label()))
            .on_click(cx.listener(move |this, _, window, cx| this.set_page(page, window, cx)))
            .into_any_element()
    }
}
