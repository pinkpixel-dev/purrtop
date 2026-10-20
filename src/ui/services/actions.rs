//! What the right-click menu does to the selected service.

use std::future::Future;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;

use super::*;
use crate::system::services::{Scope, Service};
use crate::ui::ShowProcess;

impl ServicesPage {
    fn target(&self, cx: &App) -> Option<(Service, Systemd)> {
        let table = self.table.read(cx).delegate();
        let service = table.selected.as_deref().and_then(|name| table.service(name)).cloned()?;
        Some((service, self.systemd.clone()?))
    }

    /// Runs one systemd call for the selected service. The list updates
    /// itself from systemd's signals; only failures and `done` get a notification.
    fn run<F, Fut>(&mut self, verb: &'static str, done: Option<String>, call: F, window: &mut Window, cx: &mut Context<Self>)
    where
        F: FnOnce(Systemd, String) -> Fut + 'static,
        Fut: Future<Output = Result<(), String>> + 'static,
    {
        let Some((service, systemd)) = self.target(cx) else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let result = call(systemd, service.name.clone()).await;
            _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(()) => {
                        if let Some(done) = done {
                            window.push_notification(Notification::success(done), cx);
                        }
                    }
                    Err(err) => window.push_notification(Notification::error(format!("Couldn't {verb} {}: {err}", service.name)), cx),
                }
                // Read the details and log again, since they likely changed.
                this.details = None;
                this.selection_changed(cx);
            });
        })
        .detach();
    }

    pub(super) fn start(&mut self, _: &Start, window: &mut Window, cx: &mut Context<Self>) {
        self.run("start", None, |systemd, name| async move { systemd.start(&name).await }, window, cx);
    }

    pub(super) fn stop(&mut self, _: &Stop, window: &mut Window, cx: &mut Context<Self>) {
        self.run("stop", None, |systemd, name| async move { systemd.stop(&name).await }, window, cx);
    }

    pub(super) fn restart(&mut self, _: &Restart, window: &mut Window, cx: &mut Context<Self>) {
        self.run("restart", None, |systemd, name| async move { systemd.restart(&name).await }, window, cx);
    }

    fn set_enabled(&mut self, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((service, _)) = self.target(cx) else {
            return;
        };
        let start = if self.scope == Scope::User { "at login" } else { "at boot" };
        let (verb, done) = if enabled {
            ("enable", format!("{} will start {start}", service.name))
        } else {
            ("disable", format!("{} won't start {start}", service.name))
        };
        self.run(verb, Some(done), move |systemd, name| async move { systemd.set_enabled(&name, enabled).await }, window, cx);
    }

    pub(super) fn enable(&mut self, _: &Enable, window: &mut Window, cx: &mut Context<Self>) {
        self.set_enabled(true, window, cx);
    }

    pub(super) fn disable(&mut self, _: &Disable, window: &mut Window, cx: &mut Context<Self>) {
        self.set_enabled(false, window, cx);
    }

    pub(super) fn open_in_processes(&mut self, _: &OpenInProcesses, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pid) = self.target(cx).and_then(|(service, _)| service.main_pid) {
            window.dispatch_action(Box::new(ShowProcess(pid)), cx);
        }
    }

    pub(super) fn copy_name(&mut self, _: &CopyName, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((service, _)) = self.target(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(service.name.clone()));
            window.push_notification(Notification::success(format!("Copied \"{}\"", service.name)), cx);
        }
    }
}
