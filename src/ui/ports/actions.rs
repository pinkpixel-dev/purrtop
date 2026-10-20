//! What the right-click menu and the keyboard do to the selected port.

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;

use super::*;
use crate::system::control::{self, AdminAction};
use crate::ui::ShowProcess;
use crate::ui::processes::confirm;

impl PortsPage {
    fn target(&self, cx: &App) -> Option<Port> {
        let table = self.table.read(cx).delegate();
        table.selected.and_then(|inode| table.port(inode)).cloned()
    }

    /// Asks first, then sends `signal` to the port's process, through
    /// pkexec when it belongs to someone else.
    fn signal(&mut self, signal: i32, verb: &'static str, message: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(port) = self.target(cx) else {
            return;
        };
        let Some(pid) = port.pid.filter(|&pid| pid != 1) else {
            return;
        };
        let name = port.name.clone().unwrap_or_else(|| format!("PID {pid}"));
        let title = format!("{verb} {name}?");
        let message = format!("It's listening on {} port {}. {message}", port.socket.protocol_label(), port.socket.address.port());
        let page = cx.entity().downgrade();
        confirm::confirm(
            title,
            message,
            if signal == libc::SIGKILL { "Kill process" } else { "End process" },
            move |window, cx| {
                let label = format!("{verb} {name}");
                _ = page.update(cx, |page, cx| page.send(pid, signal, label, window, cx));
            },
            window,
            cx,
        );
    }

    /// Sends the signal; when the kernel refuses, retries through pkexec off
    /// the UI thread and reports how it went.
    fn send(&mut self, pid: u32, signal: i32, label: String, window: &mut Window, cx: &mut Context<Self>) {
        if control::signal(&[pid], signal).is_ok() {
            self.refresh(cx);
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { AdminAction::Signal { pids: vec![pid], signal }.run() }).await;
            _ = this.update_in(cx, |this, window, cx| {
                let notification = match result {
                    Ok(()) => Notification::success(format!("{label}: done")),
                    Err(err) => Notification::error(format!("{label}: {err}")),
                };
                window.push_notification(notification, cx);
                this.refresh(cx);
            });
        })
        .detach();
    }

    pub(super) fn end_process(&mut self, _: &EndProcess, window: &mut Window, cx: &mut Context<Self>) {
        self.signal(libc::SIGTERM, "End", "It gets a chance to close cleanly.", window, cx);
    }

    pub(super) fn kill_process(&mut self, _: &KillProcess, window: &mut Window, cx: &mut Context<Self>) {
        self.signal(libc::SIGKILL, "Kill", "It stops right away, without a chance to save anything.", window, cx);
    }

    /// Looks up every socket's owners as root, once. Asks for your password.
    pub(super) fn find_owners(&mut self, _: &FindOwners, window: &mut Window, cx: &mut Context<Self>) {
        if self.looking_up {
            return;
        }
        self.looking_up = true;
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async { ports::owners_as_admin() }).await;
            _ = this.update_in(cx, |this, window, cx| {
                this.looking_up = false;
                match result {
                    Ok(owners) => {
                        this.admin_owners = owners;
                        this.refresh(cx);
                    }
                    Err(err) => window.push_notification(Notification::error(format!("Couldn't look up the owners: {err}")), cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_in_processes(&mut self, _: &OpenInProcesses, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(pid) = self.target(cx).and_then(|port| port.pid) {
            window.dispatch_action(Box::new(ShowProcess(pid)), cx);
        }
    }

    pub(super) fn copy_address(&mut self, _: &CopyAddress, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(port) = self.target(cx) {
            let address = port.socket.address.to_string();
            cx.write_to_clipboard(ClipboardItem::new_string(address.clone()));
            window.push_notification(Notification::success(format!("Copied \"{address}\"")), cx);
        }
    }
}
