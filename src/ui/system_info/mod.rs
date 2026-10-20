//! The System Info page: what the machine is and what it runs, one tab per topic.

mod hardware;
mod network;
mod software;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{ActiveTheme as _, StyledExt as _, v_flex};
use std::time::Duration;

use gpui_kit::*;

use super::performance::parts::facts;
use crate::system::hardware::HardwareInfo;
use crate::system::monitor::Monitor;
use crate::system::network::NetworkInfo;
use crate::system::software::SoftwareInfo;

const CONTEXT: &str = "SystemInfo";
/// How often the Network tab re-reads routes, DNS and Wi-Fi while it's showing.
const NETWORK_REFRESH: Duration = Duration::from_secs(5);

/// Label and value pairs. A `None` value leaves the row out.
type Facts = Vec<(&'static str, Option<String>)>;

actions!(system_info, [NextTab, PreviousTab]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-tab", NextTab, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum InfoTab {
    #[default]
    Hardware,
    Software,
    Network,
}

impl InfoTab {
    const ALL: [InfoTab; 3] = [InfoTab::Hardware, InfoTab::Software, InfoTab::Network];

    fn label(self) -> &'static str {
        match self {
            InfoTab::Hardware => "Hardware",
            InfoTab::Software => "Software",
            InfoTab::Network => "Network",
        }
    }
}

pub struct SystemInfoPage {
    monitor: Entity<Monitor>,
    tab: InfoTab,
    hardware: HardwareInfo,
    software: SoftwareInfo,
    /// `None` until the first read after the Network tab is shown.
    network: Option<NetworkInfo>,
    /// The refresh loop. Dropping it stops the loop.
    network_task: Option<Task<()>>,
    focus_handle: FocusHandle,
    active: bool,
    _subscription: Subscription,
}

impl SystemInfoPage {
    pub fn new(monitor: Entity<Monitor>, cx: &mut Context<Self>) -> Self {
        // Disks and batteries come and go, so follow the monitor while showing.
        let subscription = cx.observe(&monitor, |this, _, cx| {
            if this.active {
                cx.notify();
            }
        });
        Self {
            monitor,
            tab: InfoTab::default(),
            hardware: HardwareInfo::read(),
            software: SoftwareInfo::read(),
            network: None,
            network_task: None,
            focus_handle: cx.focus_handle(),
            active: false,
            _subscription: subscription,
        }
    }

    /// Showing the page hands it the keyboard, so Ctrl+Tab switches tabs right away.
    pub fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.active = active;
        if active {
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
        self.watch_network(cx);
    }

    /// Runs the network refresh loop only while the Network tab is on screen.
    fn watch_network(&mut self, cx: &mut Context<Self>) {
        if !(self.active && self.tab == InfoTab::Network) {
            self.network_task = None;
            return;
        }
        if self.network_task.is_some() {
            return;
        }
        self.network_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let info = cx.background_spawn(async { NetworkInfo::read() }).await;
                let updated = this.update(cx, |this, cx| {
                    this.network = Some(info);
                    cx.notify();
                });
                if updated.is_err() {
                    break;
                }
                cx.background_executor().timer(NETWORK_REFRESH).await;
            }
        }));
    }

    fn select(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = InfoTab::ALL[ix.min(InfoTab::ALL.len() - 1)];
        self.focus_handle.focus(window, cx);
        self.watch_network(cx);
        cx.notify();
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let tabs = InfoTab::ALL;
        let current = tabs.iter().position(|&tab| tab == self.tab).unwrap_or(0);
        let next = if forward { (current + 1) % tabs.len() } else { (current + tabs.len() - 1) % tabs.len() };
        self.tab = tabs[next];
        self.watch_network(cx);
        cx.notify();
    }

    fn next_tab(&mut self, _: &NextTab, _: &mut Window, cx: &mut Context<Self>) {
        self.step(true, cx);
    }

    fn previous_tab(&mut self, _: &PreviousTab, _: &mut Window, cx: &mut Context<Self>) {
        self.step(false, cx);
    }
}

impl Focusable for SystemInfoPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for SystemInfoPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = InfoTab::ALL.iter().position(|&tab| tab == self.tab).unwrap_or(0);
        let tabs = TabBar::new("system-info-tabs")
            .underline()
            .selected_index(selected)
            .children(InfoTab::ALL.map(|tab| Tab::new().label(tab.label())))
            .on_click(cx.listener(|this, ix: &usize, window, cx| this.select(*ix, window, cx)));

        let sections = match self.tab {
            InfoTab::Hardware => match self.monitor.read(cx).latest() {
                Some(snapshot) => hardware::render(&self.hardware, snapshot, cx),
                None => vec![hardware::render_system(&self.hardware, cx)],
            },
            InfoTab::Software => software::render(&self.software, cx),
            InfoTab::Network => network::render(self.network.as_ref(), self.monitor.read(cx).latest(), cx),
        };

        v_flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::previous_tab))
            .size_full()
            .p_4()
            .gap_3()
            .child(div().flex_shrink_0().text_lg().font_semibold().child("System Info"))
            .child(div().flex_shrink_0().child(tabs))
            .child(
                v_flex()
                    .id("system-info-body")
                    .flex_1()
                    .min_h_0()
                    .p_4()
                    .gap_6()
                    .rounded(cx.theme().radius_lg)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().group_box)
                    .overflow_y_scrollbar()
                    .children(sections),
            )
    }
}

/// A titled block of facts. Facts without a value are left out.
fn section(title: impl Into<SharedString>, items: Facts, cx: &App) -> AnyElement {
    v_flex()
        .flex_shrink_0()
        .gap_2()
        .child(div().text_sm().font_semibold().child(title.into()))
        .child(facts(known(items), cx))
        .into_any_element()
}

/// A block holding one titled group of facts per device, like each GPU or disk.
fn device_section(title: &'static str, devices: Vec<(String, Facts)>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    v_flex()
        .flex_shrink_0()
        .gap_3()
        .child(div().text_sm().font_semibold().child(title))
        .children(devices.into_iter().map(|(name, items)| {
            v_flex()
                .gap_1p5()
                .child(div().text_xs().font_semibold().text_color(theme.muted_foreground).child(name))
                .child(facts(known(items), cx))
        }))
        .into_any_element()
}

fn known(items: Facts) -> Vec<(&'static str, String)> {
    items.into_iter().filter_map(|(label, value)| Some((label, value?))).collect()
}

fn yes_no(value: bool) -> String {
    if value { "Yes" } else { "No" }.into()
}
