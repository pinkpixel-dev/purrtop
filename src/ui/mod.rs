mod about;
mod charts;
mod page;
mod performance;
mod processes;
mod sidebar;
mod status_bar;
mod summary;
mod title_bar;

use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;
use serde::Deserialize;

pub use page::Page;

use self::performance::PerformancePage;
use self::processes::ProcessesPage;

use crate::settings::Settings;
use crate::system::monitor::Monitor;
use crate::theme::{self, Themes};

const KEY_CONTEXT: &str = "Purrtop";

actions!(purrtop, [Quit, About, NextPage, PreviousPage]);

/// Switches to the theme at this index, in menu order.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = purrtop, no_json)]
pub struct SwitchTheme(pub usize);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("alt-down", NextPage, Some(KEY_CONTEXT)),
        KeyBinding::new("alt-up", PreviousPage, Some(KEY_CONTEXT)),
    ]);
    performance::init(cx);
    processes::init(cx);
    cx.on_action(|_: &Quit, cx| cx.quit());
    // App-wide so the menu bar and the status bar picker both reach it.
    cx.on_action(|action: &SwitchTheme, cx| {
        theme::apply(action.0, cx);
        crate::menus::install(cx);
    });
}

/// The main window: title bar, sidebar, the current page and the status bar.
pub struct Purrtop {
    focus_handle: FocusHandle,
    monitor: Entity<Monitor>,
    performance: Entity<PerformancePage>,
    processes: Entity<ProcessesPage>,
    menu_bar: Entity<AppMenuBar>,
    page: Page,
    _subscriptions: Vec<Subscription>,
}

impl Purrtop {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let monitor = cx.new(Monitor::new);
        let performance = cx.new(|cx| PerformancePage::new(monitor.clone(), cx));
        let processes = cx.new(|cx| ProcessesPage::new(monitor.clone(), window, cx));
        let menu_bar = AppMenuBar::new(cx);
        let subscriptions = vec![
            cx.observe(&monitor, |_, _, cx| cx.notify()),
            cx.observe_global_in::<Themes>(window, |this, window, cx| this.theme_changed(window, cx)),
        ];

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        Self { focus_handle, monitor, performance, processes, menu_bar, page: Page::default(), _subscriptions: subscriptions }
    }

    /// Refreshes the menu check marks and remembers the pick for next launch.
    fn theme_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_bar.update(cx, |bar, cx| bar.reload(cx));
        let settings = Settings { theme: Some(Themes::global(cx).current_name().to_string()) };
        if let Err(err) = settings.save() {
            window.push_notification(Notification::error(format!("Couldn't save the theme: {err}")), cx);
        }
    }

    fn set_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        self.page = page;
        self.performance.update(cx, |performance, cx| performance.set_active(page == Page::Performance, window, cx));
        self.processes.update(cx, |processes, cx| processes.set_active(page == Page::Processes, cx));
        cx.notify();
    }

    fn step_page(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let pages = Page::ALL;
        let current = pages.iter().position(|&page| page == self.page).unwrap_or(0);
        let next = if forward { (current + 1) % pages.len() } else { (current + pages.len() - 1) % pages.len() };
        self.set_page(pages[next], window, cx);
    }

    fn next_page(&mut self, _: &NextPage, window: &mut Window, cx: &mut Context<Self>) {
        self.step_page(true, window, cx);
    }

    fn previous_page(&mut self, _: &PreviousPage, window: &mut Window, cx: &mut Context<Self>) {
        self.step_page(false, window, cx);
    }

    fn about(&mut self, _: &About, window: &mut Window, cx: &mut Context<Self>) {
        about::open(window, cx);
    }

    fn render_page(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        match self.page {
            Page::Summary => summary::render(self.monitor.read(cx), window, cx),
            Page::Performance => self.performance.clone().into_any_element(),
            Page::Processes => self.processes.clone().into_any_element(),
        }
    }
}

impl Focusable for Purrtop {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Purrtop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::next_page))
            .on_action(cx.listener(Self::previous_page))
            .on_action(cx.listener(Self::about))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.render_title_bar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(cx))
                    .child(div().flex_1().min_w_0().h_full().child(self.render_page(window, cx))),
            )
            .child(self.render_status_bar(cx))
    }
}
