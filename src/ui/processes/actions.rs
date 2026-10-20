//! What the right-click menu and the keyboard do to the selected process.

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;

use super::rows::ViewMode;
use super::*;
use crate::system::control::{self, AdminAction, Denied};
use crate::system::processes::ProcessInfo;

impl ProcessesPage {
    /// The selected process, when it's still running.
    fn target(&self, cx: &App) -> Option<ProcessInfo> {
        let pid = self.selected(cx)?;
        self.table.read(cx).delegate().process(pid).cloned()
    }

    /// Runs `direct`; when the kernel refuses, retries as `admin` through
    /// pkexec off the UI thread and reports how it went.
    fn run(&mut self, label: String, direct: Result<(), Denied>, admin: AdminAction, window: &mut Window, cx: &mut Context<Self>) {
        if direct.is_ok() {
            return;
        }
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { admin.run() }).await;
            _ = this.update_in(cx, |_, window, cx| {
                let notification = match result {
                    Ok(()) => Notification::success(format!("{label}: done")),
                    Err(err) => Notification::error(format!("{label}: {err}")),
                };
                window.push_notification(notification, cx);
            });
        })
        .detach();
    }

    fn send(&mut self, pids: Vec<u32>, signal: i32, label: String, window: &mut Window, cx: &mut Context<Self>) {
        let direct = control::signal(&pids, signal);
        self.run(label, direct, AdminAction::Signal { pids, signal }, window, cx);
    }

    /// Asks first, then sends `signal` to `pids`.
    fn send_confirmed(&mut self, pids: Vec<u32>, signal: i32, title: String, message: String, button: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        let page = cx.entity().downgrade();
        let label = title.clone();
        confirm::confirm(
            title,
            message,
            button,
            move |window, cx| {
                _ = page.update(cx, |page, cx| page.send(pids.clone(), signal, label.clone(), window, cx));
            },
            window,
            cx,
        );
    }

    pub(super) fn terminate(&mut self, _: &Terminate, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(process) = self.target(cx) {
            self.send(vec![process.pid], libc::SIGTERM, format!("Terminate {}", process.name), window, cx);
        }
    }

    pub(super) fn force_terminate(&mut self, _: &ForceTerminate, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(process) = self.target(cx) {
            self.send_confirmed(
                vec![process.pid],
                libc::SIGKILL,
                format!("Force terminate {}?", process.name),
                "It stops right away, without a chance to save anything.".into(),
                "Force terminate",
                window,
                cx,
            );
        }
    }

    fn tree(&self, process: &ProcessInfo, cx: &App) -> Vec<u32> {
        control::tree(self.table.read(cx).delegate().processes(), process.pid)
    }

    pub(super) fn end_tree(&mut self, _: &EndTree, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(process) = self.target(cx) {
            let pids = self.tree(&process, cx);
            let message = format!("This asks {} process{} to close.", pids.len(), if pids.len() == 1 { "" } else { "es" });
            self.send_confirmed(pids, libc::SIGTERM, format!("End {} and its children?", process.name), message, "End tree", window, cx);
        }
    }

    pub(super) fn force_quit_tree(&mut self, _: &ForceQuitTree, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(process) = self.target(cx) {
            let pids = self.tree(&process, cx);
            let message = format!("{} process{} stop right away, without a chance to save anything.", pids.len(), if pids.len() == 1 { "" } else { "es" });
            self.send_confirmed(pids, libc::SIGKILL, format!("Force quit {} and its children?", process.name), message, "Force quit", window, cx);
        }
    }

    pub(super) fn suspend(&mut self, _: &Suspend, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(process) = self.target(cx) {
            self.send(vec![process.pid], libc::SIGSTOP, format!("Suspend {}", process.name), window, cx);
        }
    }

    pub(super) fn resume(&mut self, _: &Resume, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(process) = self.target(cx) {
            self.send(vec![process.pid], libc::SIGCONT, format!("Resume {}", process.name), window, cx);
        }
    }

    pub(super) fn send_signal(&mut self, action: &SendSignal, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(process) = self.target(cx) {
            self.send(vec![process.pid], action.0, format!("Signal {} to {}", action.0, process.name), window, cx);
        }
    }

    pub(super) fn set_nice(&mut self, action: &SetNice, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(process) = self.target(cx) {
            let direct = control::renice(process.pid, action.0);
            self.run(format!("Set {} to nice {}", process.name, action.0), direct, AdminAction::Renice { pid: process.pid, nice: action.0 }, window, cx);
        }
    }

    pub(super) fn set_affinity(&mut self, _: &SetAffinity, window: &mut Window, cx: &mut Context<Self>) {
        let Some(process) = self.target(cx) else {
            return;
        };
        let cpu_count = self.monitor.read(cx).latest().map_or(1, |snapshot| snapshot.cores.len().max(1));
        let current = control::affinity(process.pid).unwrap_or_else(|| (0..cpu_count).collect());
        let page = cx.entity().downgrade();
        let (pid, name) = (process.pid, process.name.clone());
        affinity::open(
            process.name,
            cpu_count,
            current,
            move |cpus, window, cx| {
                let direct = control::set_affinity(pid, &cpus);
                let label = format!("Set CPUs for {name}");
                _ = page.update(cx, |page, cx| page.run(label, direct, AdminAction::Affinity { pid, cpus }, window, cx));
            },
            window,
            cx,
        );
    }

    pub(super) fn create_sample(&mut self, _: &CreateSample, window: &mut Window, cx: &mut Context<Self>) {
        let Some(process) = self.target(cx) else {
            return;
        };
        window.push_notification(Notification::info(format!("Sampling {}…", process.name)), cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { control::sample(process.pid, &process.name) }).await;
            _ = this.update_in(cx, |_, window, cx| match result {
                Ok(path) => {
                    window.push_notification(Notification::success(format!("Sample saved to {}", path.display())), cx);
                    cx.open_url(&format!("file://{}", path.display()));
                }
                Err(err) => window.push_notification(Notification::error(format!("Couldn't sample: {err}")), cx),
            });
        })
        .detach();
    }

    pub(super) fn open_file_location(&mut self, _: &OpenFileLocation, window: &mut Window, cx: &mut Context<Self>) {
        let Some(exe) = self.target(cx).and_then(|process| process.exe) else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { control::show_in_file_manager(&exe) }).await;
            if let Err(err) = result {
                _ = this.update_in(cx, |_, window, cx| window.push_notification(Notification::error(err), cx));
            }
        })
        .detach();
    }

    pub(super) fn search_online(&mut self, _: &SearchOnline, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(process) = self.target(cx) {
            cx.open_url(&control::search_url(&process.name));
        }
    }

    pub(super) fn copy_name(&mut self, _: &CopyName, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(process) = self.target(cx) {
            cx.write_to_clipboard(ClipboardItem::new_string(process.name.clone()));
            window.push_notification(Notification::success(format!("Copied \"{}\"", process.name)), cx);
        }
    }

    pub(super) fn toggle_follow(&mut self, _: &ToggleFollow, _: &mut Window, cx: &mut Context<Self>) {
        let selected = self.selected(cx);
        self.update_table(cx, |table| table.follow = if table.follow == selected { None } else { selected });
    }

    pub(super) fn expand_all(&mut self, _: &ExpandAll, _: &mut Window, cx: &mut Context<Self>) {
        self.update_table(cx, |table| table.expand_all());
    }

    pub(super) fn collapse_all(&mut self, _: &CollapseAll, _: &mut Window, cx: &mut Context<Self>) {
        self.update_table(cx, |table| {
            table.expanded.clear();
            table.rebuild();
        });
    }

    pub(super) fn set_flat_view(&mut self, action: &SetFlatView, _: &mut Window, cx: &mut Context<Self>) {
        let mode = if action.0 { ViewMode::Flat } else { ViewMode::Tree };
        self.update_table(cx, |table| {
            table.mode = mode;
            table.rebuild();
        });
    }

    pub(super) fn show_memory_percent(&mut self, action: &ShowMemoryPercent, _: &mut Window, cx: &mut Context<Self>) {
        self.update_table(cx, |table| table.memory_percent = action.0);
    }

    pub(super) fn show_details(&mut self, _: &ShowDetails, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(process) = self.target(cx) {
            details::open(process, window, cx);
        }
    }
}
