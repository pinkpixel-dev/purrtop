//! The File / View / Help menu bar. Rebuilt whenever the theme changes so the
//! check mark follows it.

use gpui_kit::component::GlobalState;
use gpui_kit::*;

use crate::app_info;
use crate::theme::Themes;
use crate::ui::{About, Quit, RunTask, SwitchTheme};

pub fn install(cx: &mut App) {
    let themes = Themes::global(cx);
    let current = themes.current();
    let theme_items: Vec<MenuItem> = themes
        .names()
        .enumerate()
        .map(|(ix, name)| MenuItem::action(name.clone(), SwitchTheme(ix)).checked(ix == current))
        .collect();

    let menus = [
        Menu::new("File").items([MenuItem::action("Run new task…", RunTask), MenuItem::separator(), MenuItem::action("Quit", Quit)]),
        Menu::new("View").items([MenuItem::submenu(Menu::new("Theme").items(theme_items))]),
        Menu::new("Help").items([MenuItem::action(format!("About {}", app_info::NAME), About)]),
    ];
    GlobalState::global_mut(cx).set_app_menus(menus.into_iter().map(Menu::owned).collect());
}
