mod about;
mod charts;
mod disk_space;
mod page;
mod performance;
mod ports;
mod processes;
mod run_task;
mod services;
mod settings_dialog;
mod sidebar;
mod startup;
mod status_bar;
mod summary;
mod system_info;
mod title_bar;
mod users;

use gpui_kit::component::menu::AppMenuBar;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;
use serde::Deserialize;

pub use page::Page;

use self::disk_space::DiskSpacePage;
use self::performance::PerformancePage;
use self::ports::PortsPage;
use self::processes::ProcessesPage;
use self::services::ServicesPage;
use self::startup::StartupPage;
use self::system_info::SystemInfoPage;
use self::users::UsersPage;

use crate::settings::Settings;
use crate::system::monitor::Monitor;
use crate::theme::{self, Themes};

const KEY_CONTEXT: &str = "Purrtop";

actions!(purrtop, [Quit, About, NextPage, PreviousPage, RunTask, OpenSettings]);

/// Switches to the theme at this index, in menu order.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = purrtop, no_json)]
pub struct SwitchTheme(pub usize);

/// Switches to the Processes page with this PID selected.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = purrtop, no_json)]
pub struct ShowProcess(pub u32);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-q", Quit, None),
        KeyBinding::new("ctrl-n", RunTask, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-,", OpenSettings, Some(KEY_CONTEXT)),
        KeyBinding::new("alt-down", NextPage, Some(KEY_CONTEXT)),
        KeyBinding::new("alt-up", PreviousPage, Some(KEY_CONTEXT)),
    ]);
    performance::init(cx);
    processes::init(cx);
    system_info::init(cx);
    startup::init(cx);
    users::init(cx);
    services::init(cx);
    ports::init(cx);
    disk_space::init(cx);
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
    system_info: Entity<SystemInfoPage>,
    startup: Entity<StartupPage>,
    users: Entity<UsersPage>,
    services: Entity<ServicesPage>,
    ports: Entity<PortsPage>,
    disk_space: Entity<DiskSpacePage>,
    menu_bar: Entity<AppMenuBar>,
    page: Page,
    _subscriptions: Vec<Subscription>,
}

impl Purrtop {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let monitor = cx.new(Monitor::new);
        monitor.read(cx).set_nvidia(Settings::load().nvidia);
        let performance = cx.new(|cx| PerformancePage::new(monitor.clone(), cx));
        let processes = cx.new(|cx| ProcessesPage::new(monitor.clone(), window, cx));
        let system_info = cx.new(|cx| SystemInfoPage::new(monitor.clone(), cx));
        let startup = cx.new(|cx| StartupPage::new(window, cx));
        let users = cx.new(|cx| UsersPage::new(monitor.clone(), window, cx));
        let services = cx.new(|cx| ServicesPage::new(window, cx));
        let ports = cx.new(|cx| PortsPage::new(monitor.clone(), window, cx));
        let disk_space = cx.new(|cx| DiskSpacePage::new(window, cx));
        let menu_bar = AppMenuBar::new(cx);
        let subscriptions = vec![
            cx.observe(&monitor, |_, _, cx| cx.notify()),
            cx.observe_global_in::<Themes>(window, |this, window, cx| this.theme_changed(window, cx)),
        ];

        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);

        Self { focus_handle, monitor, performance, processes, system_info, startup, users, services, ports, disk_space, menu_bar, page: Page::default(), _subscriptions: subscriptions }
    }

    /// Refreshes the menu check marks and remembers the pick for next launch.
    fn theme_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu_bar.update(cx, |bar, cx| bar.reload(cx));
        let mut settings = Settings::load();
        settings.theme = Some(Themes::global(cx).current_name().to_string());
        if let Err(err) = settings.save() {
            window.push_notification(Notification::error(format!("Couldn't save the theme: {err}")), cx);
        }
    }

    fn set_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        self.page = page;
        self.performance.update(cx, |performance, cx| performance.set_active(page == Page::Performance, window, cx));
        self.processes.update(cx, |processes, cx| processes.set_active(page == Page::Processes, cx));
        self.system_info.update(cx, |system_info, cx| system_info.set_active(page == Page::SystemInfo, window, cx));
        self.startup.update(cx, |startup, cx| startup.set_active(page == Page::Startup, window, cx));
        self.users.update(cx, |users, cx| users.set_active(page == Page::Users, window, cx));
        self.services.update(cx, |services, cx| services.set_active(page == Page::Services, window, cx));
        self.ports.update(cx, |ports, cx| ports.set_active(page == Page::Ports, window, cx));
        self.disk_space.update(cx, |disk_space, cx| disk_space.set_active(page == Page::DiskSpace, window, cx));
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

    fn show_process(&mut self, action: &ShowProcess, window: &mut Window, cx: &mut Context<Self>) {
        self.set_page(Page::Processes, window, cx);
        self.processes.update(cx, |processes, cx| processes.reveal(action.0, cx));
    }

    fn run_task(&mut self, _: &RunTask, window: &mut Window, cx: &mut Context<Self>) {
        run_task::open(window, cx);
    }

    fn open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        settings_dialog::open(self.monitor.clone(), window, cx);
    }

    fn render_page(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        match self.page {
            Page::Summary => summary::render(self.monitor.read(cx), window, cx),
            Page::Performance => self.performance.clone().into_any_element(),
            Page::Processes => self.processes.clone().into_any_element(),
            Page::SystemInfo => self.system_info.clone().into_any_element(),
            Page::Startup => self.startup.clone().into_any_element(),
            Page::Users => self.users.clone().into_any_element(),
            Page::Services => self.services.clone().into_any_element(),
            Page::Ports => self.ports.clone().into_any_element(),
            Page::DiskSpace => self.disk_space.clone().into_any_element(),
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
            .on_action(cx.listener(Self::run_task))
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::show_process))
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
