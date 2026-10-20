//! Adding, removing and cleaning up your own autostart entries.

use std::collections::HashSet;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;

use super::*;
use crate::system::launchers::{self, Launcher};
use crate::ui::processes::confirm;

impl StartupPage {
    /// Your entries that point at a file or program that's gone.
    pub(super) fn broken(&self, cx: &App) -> Vec<Entry> {
        self.table.read(cx).delegate().entries().iter().filter(|entry| entry.is_broken() && entry.can_remove()).cloned().collect()
    }

    pub(super) fn remove(&mut self, _: &Remove, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.target(cx).filter(Entry::can_remove) else {
            return;
        };
        let message = if entry.link.is_some() {
            "This deletes the link in your autostart folder. The app stays installed."
        } else {
            "Its file goes to the Trash, so you can restore it later."
        };
        let title = format!("Remove {} from startup?", entry.name);
        let page = cx.entity().downgrade();
        confirm::confirm(
            title,
            message,
            "Remove",
            move |window, cx| {
                let entries = vec![entry.clone()];
                _ = page.update(cx, |page, cx| page.remove_entries(entries, window, cx));
            },
            window,
            cx,
        );
    }

    pub(super) fn clean_up(&mut self, _: &CleanUp, window: &mut Window, cx: &mut Context<Self>) {
        let broken = self.broken(cx);
        if broken.is_empty() {
            return;
        }
        let title = match broken.len() {
            1 => "Remove 1 broken entry?".to_string(),
            count => format!("Remove {count} broken entries?"),
        };
        let names: Vec<&str> = broken.iter().map(|entry| entry.name.as_str()).collect();
        let message = format!("{}. They point at apps or files that are gone, so they can't start anyway.", names.join(", "));
        let page = cx.entity().downgrade();
        confirm::confirm(
            title,
            message,
            "Remove",
            move |window, cx| {
                let entries = broken.clone();
                _ = page.update(cx, |page, cx| page.remove_entries(entries, window, cx));
            },
            window,
            cx,
        );
    }

    fn remove_entries(&mut self, entries: Vec<Entry>, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let results: Vec<(String, Result<(), String>)> =
                cx.background_spawn(async move { entries.into_iter().map(|entry| (entry.name.clone(), autostart::remove(&entry))).collect() }).await;
            _ = this.update_in(cx, |this, window, cx| {
                let failures: Vec<String> = results.iter().filter_map(|(name, result)| result.as_ref().err().map(|err| format!("{name}: {err}"))).collect();
                let notification = match (failures.is_empty(), results.as_slice()) {
                    (false, _) => Notification::error(format!("Couldn't remove {}", failures.join("; "))),
                    (true, [(name, _)]) => Notification::success(format!("Removed {name} from startup")),
                    (true, results) => Notification::success(format!("Removed {} entries", results.len())),
                };
                window.push_notification(notification, cx);
                this.reload(cx);
            });
        })
        .detach();
    }

    pub(super) fn open_add(&mut self, _: &AddApp, window: &mut Window, cx: &mut Context<Self>) {
        let taken: HashSet<String> = self.table.read(cx).delegate().entries().iter().map(|entry| entry.id.clone()).collect();
        let page = cx.entity().downgrade();
        add_dialog::open(
            taken,
            move |launcher, window, cx| {
                _ = page.update(cx, |page, cx| page.add(launcher, window, cx));
            },
            window,
            cx,
        );
    }

    fn add(&mut self, launcher: Launcher, window: &mut Window, cx: &mut Context<Self>) {
        let notification = match launchers::add(&launcher) {
            Ok(()) => Notification::success(format!("{} will start at your next login", launcher.name)),
            Err(err) => Notification::error(format!("Couldn't add {}: {err}", launcher.name)),
        };
        self.update_table(cx, |table| table.selected = Some(launcher.id.clone()));
        window.push_notification(notification, cx);
        self.reload(cx);
    }
}
