use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::*;

use super::{Purrtop, SwitchTheme};
use crate::system::format;
use crate::theme::Themes;

impl Purrtop {
    pub(super) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let monitor = self.monitor.read(cx);
        let (dot, state) = match (&monitor.error, monitor.latest()) {
            (Some(err), _) => (theme.danger, err.clone()),
            (None, Some(snapshot)) => (theme.success, format!("Live · {} processes", snapshot.processes.len()).into()),
            (None, None) => (theme.muted_foreground, SharedString::from("Starting…")),
        };
        let readings = monitor.latest().map(|snapshot| {
            format!(
                "CPU {} · Memory {} / {} · Up {}",
                format::percent(snapshot.cpu),
                format::bytes(snapshot.memory.used()),
                format::bytes(snapshot.memory.total),
                format::uptime(snapshot.uptime),
            )
        });

        StatusBar::new()
            .left(h_flex().gap_1p5().items_center().child(div().size_2().rounded_full().bg(dot)).child(state))
            .children(readings)
            .right(
                Button::new("theme-picker")
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(CatalogIcon::Palette))
                    .label(Themes::global(cx).current_name())
                    .tooltip("Theme")
                    .dropdown_menu(|mut menu, _, cx| {
                        let themes = Themes::global(cx);
                        let current = themes.current();
                        for (ix, name) in themes.names().enumerate() {
                            menu = menu.menu_with_check(name.clone(), ix == current, Box::new(SwitchTheme(ix)));
                        }
                        menu
                    }),
            )
    }
}
