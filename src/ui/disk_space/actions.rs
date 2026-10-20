//! What the right-click menu and the keyboard do to the selected file or folder.

use std::path::PathBuf;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::*;

use super::parts::{display_path, file_name};
use super::*;
use crate::system::control;
use crate::system::format;
use crate::ui::processes::confirm;

/// The selected file or folder and what trashing it would free.
#[derive(Clone)]
struct Target {
    path: PathBuf,
    size: u64,
    files: u64,
    /// `Some` for a file, with its extension.
    extension: Option<String>,
}

impl DiskSpacePage {
    fn target(&self, cx: &App) -> Option<Target> {
        match self.tab {
            SpaceTab::Files => self.files.read(cx).delegate().selected_file().map(|file| Target { path: file.path.clone(), size: file.size, files: 1, extension: Some(file.extension.clone()) }),
            SpaceTab::Folders => self.folders.read(cx).delegate().selected_folder().map(|folder| Target { path: folder.path.clone(), size: folder.size, files: folder.files, extension: None }),
            SpaceTab::Types => None,
        }
    }

    pub(super) fn open_location(&mut self, _: &OpenLocation, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.target(cx) else {
            return;
        };
        cx.spawn_in(window, async move |_, cx| {
            let path = target.path.to_string_lossy().into_owned();
            let result = cx.background_spawn(async move { control::show_in_file_manager(&path) }).await;
            if let Err(err) = result {
                _ = cx.update(|window, cx| window.push_notification(Notification::error(err), cx));
            }
        })
        .detach();
    }

    pub(super) fn copy_path(&mut self, _: &CopyPath, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(target) = self.target(cx) {
            let path = target.path.to_string_lossy().into_owned();
            cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
            window.push_notification(Notification::success(format!("Copied \"{path}\"")), cx);
        }
    }

    /// Asks first, then moves the selection to the Trash and takes it off
    /// the lists and totals.
    pub(super) fn move_to_trash(&mut self, _: &MoveToTrash, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.target(cx) else {
            return;
        };
        let what = if target.extension.is_some() { "file" } else { "folder" };
        let title = format!("Move {} to the Trash?", file_name(&target.path));
        let message = format!("This {what} takes up {} in {}. You can restore it from the Trash.", format::bytes(target.size), display_path(target.path.parent().unwrap_or(&target.path)));
        let page = cx.entity().downgrade();
        confirm::confirm(
            title,
            message,
            "Move to Trash",
            move |window, cx| {
                let target = target.clone();
                _ = page.update(cx, |page, cx| page.trash(target, window, cx));
            },
            window,
            cx,
        );
    }

    fn trash(&mut self, target: Target, window: &mut Window, cx: &mut Context<Self>) {
        cx.spawn_in(window, async move |this, cx| {
            let path = target.path.clone();
            let result = cx.background_spawn(async move { disk_scan::move_to_trash(&path) }).await;
            _ = this.update_in(cx, |this, window, cx| match result {
                Ok(()) => {
                    this.forget(&target, cx);
                    window.push_notification(Notification::success(format!("Moved {} to the Trash", file_name(&target.path))), cx);
                }
                Err(err) => window.push_notification(Notification::error(format!("Couldn't move it to the Trash: {err}")), cx),
            });
        })
        .detach();
    }

    /// Takes a trashed file or folder off every list and the totals.
    fn forget(&mut self, target: &Target, cx: &mut Context<Self>) {
        let is_file = target.extension.is_some();
        self.files.update(cx, |state, cx| {
            state.delegate_mut().remove(&target.path);
            cx.notify();
        });
        self.folders.update(cx, |state, cx| {
            state.delegate_mut().remove(&target.path, target.size, target.files, is_file);
            cx.notify();
        });
        self.types.update(cx, |state, cx| {
            state.delegate_mut().remove(target.extension.as_deref(), target.size, target.files);
            cx.notify();
        });
        if let Some(summary) = &mut self.summary {
            summary.counts.bytes = summary.counts.bytes.saturating_sub(target.size);
            summary.counts.files = summary.counts.files.saturating_sub(target.files);
        }
        cx.notify();
    }

    /// Jumps to Largest files, filtered to the selected type's extension.
    pub(super) fn show_largest_of_type(&mut self, _: &ShowLargestOfType, window: &mut Window, cx: &mut Context<Self>) {
        let Some(extension) = self.types.read(cx).delegate().selected_type().map(|total| total.extension.clone()).filter(|extension| !extension.is_empty()) else {
            return;
        };
        self.filter.update(cx, |filter, cx| filter.set_value(format!(".{extension}"), window, cx));
        self.set_filter(format!(".{extension}"), cx);
        self.select_tab(SpaceTab::Files, window, cx);
    }
}
