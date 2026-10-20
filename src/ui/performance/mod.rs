//! The Performance page: a list of devices with sparklines on the left, and
//! charts and stats for the selected one on the right.

mod cpu;
mod devices;
mod memory;
mod parts;

use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::*;

use self::devices::Device;
use super::charts::Palette;
use crate::system::monitor::Monitor;

const CONTEXT: &str = "Performance";
/// Below this window width the device list drops its sparklines.
const COMPACT_LAYOUT: f32 = 1100.;

actions!(performance, [SelectNextDevice, SelectPreviousDevice]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNextDevice, Some(CONTEXT)),
        KeyBinding::new("up", SelectPreviousDevice, Some(CONTEXT)),
    ]);
}

pub struct PerformancePage {
    monitor: Entity<Monitor>,
    device: Device,
    focus_handle: FocusHandle,
    active: bool,
    _subscription: Subscription,
}

impl PerformancePage {
    pub fn new(monitor: Entity<Monitor>, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&monitor, |this, _, cx| {
            if this.active {
                cx.notify();
            }
        });
        Self { monitor, device: Device::default(), focus_handle: cx.focus_handle(), active: false, _subscription: subscription }
    }

    /// Called when the page is shown or hidden. Showing it hands it the
    /// keyboard, so the arrow keys move through the device list right away.
    pub fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.active = active;
        if active {
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    fn select(&mut self, device: Device, window: &mut Window, cx: &mut Context<Self>) {
        self.device = device;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let all = Device::ALL;
        let current = all.iter().position(|&device| device == self.device).unwrap_or(0);
        let next = if forward { (current + 1).min(all.len() - 1) } else { current.saturating_sub(1) };
        self.device = all[next];
        cx.notify();
    }

    fn select_next(&mut self, _: &SelectNextDevice, _: &mut Window, cx: &mut Context<Self>) {
        self.step(true, cx);
    }

    fn select_previous(&mut self, _: &SelectPreviousDevice, _: &mut Window, cx: &mut Context<Self>) {
        self.step(false, cx);
    }
}

impl Focusable for PerformancePage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PerformancePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let monitor = self.monitor.read(cx);
        let page = v_flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .size_full()
            .p_4()
            .gap_3()
            .child(div().flex_shrink_0().text_lg().font_semibold().child("Performance"));

        let Some(snapshot) = monitor.latest() else {
            return page.child(
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(monitor.error.clone().unwrap_or_else(|| "Taking the first reading…".into())),
            );
        };

        let palette = Palette::new(cx);
        let viewport = window.viewport_size();
        let compact = f32::from(viewport.width) < COMPACT_LAYOUT;
        let focused = self.focus_handle.is_focused(window);
        let list = devices::render(self.device, focused, compact, monitor, snapshot, &palette, cx, |device| {
            Box::new(cx.listener(move |this, _, window, cx| this.select(device, window, cx)))
        });
        let detail = match self.device {
            Device::Cpu => cpu::render(monitor, snapshot, &palette, viewport, compact, cx),
            Device::Memory => memory::render(monitor, snapshot, &palette, viewport, cx),
        };

        page.child(
            h_flex()
                .flex_1()
                .min_h_0()
                .gap_3()
                .items_start()
                .child(list)
                .child(
                    v_flex()
                        .id("performance-detail")
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .p_3()
                        .gap_3()
                        .rounded(cx.theme().radius_lg)
                        .border_1()
                        .border_color(cx.theme().border)
                        .bg(cx.theme().group_box)
                        .overflow_y_scrollbar()
                        .child(detail),
                ),
        )
    }
}
