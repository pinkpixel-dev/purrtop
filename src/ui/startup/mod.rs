//! The Startup apps page: XDG autostart entries, with a switch to turn each
//! one on or off for the next login, a menu to start or stop it now, and
//! adding, removing and cleaning up your own entries.

mod add_dialog;
mod detail_strip;
mod manage;
mod table;

use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::table::{DataTable, TableState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, StyledExt as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use self::table::StartupTable;
use crate::system::autostart::{self, Entry};
use crate::system::control;

const CONTEXT: &str = "Startup";
/// Only while the table has focus.
const TABLE_CONTEXT: &str = "Startup > DataTable";

actions!(
    startup,
    [ToggleEnabled, StartNow, StopNow, Remove, CleanUp, AddApp, OpenFileLocation, CopyCommand, Refresh, SelectNext, SelectPrevious, SelectFirst, SelectLast]
);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some(TABLE_CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(TABLE_CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(TABLE_CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(TABLE_CONTEXT)),
        KeyBinding::new("space", ToggleEnabled, Some(TABLE_CONTEXT)),
        KeyBinding::new("delete", Remove, Some(TABLE_CONTEXT)),
        KeyBinding::new("f5", Refresh, Some(CONTEXT)),
    ]);
}

pub struct StartupPage {
    table: Entity<TableState<StartupTable>>,
    loading: bool,
}

impl StartupPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let page = cx.weak_entity();
        let table = cx.new(|cx| TableState::new(StartupTable::new(page), window, cx).row_selectable(false).col_movable(false));
        Self { table, loading: false }
    }

    /// Entries are read each time the page is shown, since they change rarely.
    pub fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if active {
            self.table.read(cx).focus_handle(cx).focus(window, cx);
            self.reload(cx);
        }
    }

    /// Reads the entries and their unit states off the UI thread.
    fn reload(&mut self, cx: &mut Context<Self>) {
        if self.loading {
            return;
        }
        self.loading = true;
        cx.spawn(async move |this, cx| {
            let entries = cx.background_spawn(async { autostart::read() }).await;
            _ = this.update(cx, |this, cx| {
                this.loading = false;
                this.update_table(cx, |table| table.set_entries(entries));
            });
        })
        .detach();
        cx.notify();
    }

    fn update_table(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut StartupTable)) {
        self.table.update(cx, |state, cx| {
            f(state.delegate_mut());
            cx.notify();
        });
        cx.notify();
    }

    fn target(&self, cx: &App) -> Option<Entry> {
        let table = self.table.read(cx).delegate();
        table.selected.as_deref().and_then(|id| table.entry(id)).cloned()
    }

    /// Turns an entry on or off for the next login. Called by the row switches too.
    pub fn set_enabled(&mut self, id: &str, enabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.table.read(cx).delegate().entry(id).cloned() else {
            return;
        };
        self.update_table(cx, |table| table.selected = Some(entry.id.clone()));
        let notification = match autostart::set_enabled(&entry, enabled) {
            Ok(()) if enabled => Notification::success(format!("{} will start at your next login", entry.name)),
            Ok(()) => Notification::success(format!("{} won't start at your next login", entry.name)),
            Err(err) => Notification::error(format!("Couldn't change {}: {err}", entry.name)),
        };
        window.push_notification(notification, cx);
        self.reload(cx);
    }

    fn toggle_enabled(&mut self, _: &ToggleEnabled, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.target(cx) {
            self.set_enabled(&entry.id, !entry.enabled, window, cx);
        }
    }

    fn run_unit(&mut self, start: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.target(cx).filter(|entry| if start { entry.can_start() } else { entry.can_stop() }) else {
            return;
        };
        cx.spawn_in(window, async move |this, cx| {
            let name = entry.name.clone();
            let result = cx.background_spawn(async move { autostart::start(&entry, start) }).await;
            _ = this.update_in(cx, |this, window, cx| {
                if let Err(err) = result {
                    let verb = if start { "start" } else { "stop" };
                    window.push_notification(Notification::error(format!("Couldn't {verb} {name}: {err}")), cx);
                }
                this.reload(cx);
            });
        })
        .detach();
    }

    fn start_now(&mut self, _: &StartNow, window: &mut Window, cx: &mut Context<Self>) {
        self.run_unit(true, window, cx);
    }

    fn stop_now(&mut self, _: &StopNow, window: &mut Window, cx: &mut Context<Self>) {
        self.run_unit(false, window, cx);
    }

    fn open_file_location(&mut self, _: &OpenFileLocation, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.target(cx) else {
            return;
        };
        let path = entry.path.to_string_lossy().into_owned();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_spawn(async move { control::show_in_file_manager(&path) }).await;
            if let Err(err) = result {
                _ = this.update_in(cx, |_, window, cx| window.push_notification(Notification::error(err), cx));
            }
        })
        .detach();
    }

    fn copy_command(&mut self, _: &CopyCommand, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(exec) = self.target(cx).and_then(|entry| entry.exec) {
            cx.write_to_clipboard(ClipboardItem::new_string(exec));
            window.push_notification(Notification::success("Copied the command"), cx);
        }
    }

    fn refresh(&mut self, _: &Refresh, _: &mut Window, cx: &mut Context<Self>) {
        self.reload(cx);
    }

    /// Moves the selection to a row, skipping the group row in the direction of travel.
    fn select_row(&mut self, row: usize, forward: bool, cx: &mut Context<Self>) {
        self.table.update(cx, |state, cx| {
            let table = state.delegate_mut();
            let rows = table.rows().len();
            if rows == 0 {
                return;
            }
            let mut row = row.min(rows - 1);
            while table.id_at(row).is_none() {
                match (forward, row) {
                    (true, r) if r + 1 < rows => row += 1,
                    (false, r) if r > 0 => row -= 1,
                    _ => return,
                }
            }
            table.selected = table.id_at(row);
            state.vertical_scroll_handle.scroll_to_item(row, ScrollStrategy::Nearest);
            cx.notify();
        });
        cx.notify();
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let table = self.table.read(cx).delegate();
        let row = match table.selected.as_deref().and_then(|id| table.row_of(id)) {
            Some(row) => row.saturating_add_signed(delta),
            None => 0,
        };
        self.select_row(row, delta >= 0, cx);
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    fn select_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        self.select_row(0, true, cx);
    }

    fn select_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        let last = self.table.read(cx).delegate().rows().len().saturating_sub(1);
        self.select_row(last, false, cx);
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let starting = self.table.read(cx).delegate().entries().iter().filter(|entry| entry.starts_here()).count();
        let broken = self.broken(cx).len();
        h_flex()
            .gap_3()
            .items_center()
            .child(div().text_lg().font_semibold().child("Startup apps"))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{starting} start at login")))
            .child(div().flex_1())
            .when(broken > 0, |this| {
                this.child(
                    Button::new("startup-clean-up")
                        .ghost()
                        .small()
                        .icon(Icon::new(CatalogIcon::BrushCleaning))
                        .label(format!("Clean up ({broken})"))
                        .tooltip("Remove your entries for apps or files that are gone")
                        .on_click(cx.listener(|this, _, window, cx| this.clean_up(&CleanUp, window, cx))),
                )
            })
            .child(
                Button::new("startup-add")
                    .ghost()
                    .small()
                    .icon(Icon::new(CatalogIcon::Plus))
                    .label("Add")
                    .tooltip("Start an installed app at login")
                    .on_click(cx.listener(|this, _, window, cx| this.open_add(&AddApp, window, cx))),
            )
            .child(
                Button::new("startup-refresh")
                    .ghost()
                    .small()
                    .icon(Icon::new(CatalogIcon::RefreshCw))
                    .label("Refresh")
                    .loading(self.loading)
                    .disabled(self.loading)
                    .tooltip("Read the autostart folders again (F5)")
                    .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
            )
    }
}

impl Render for StartupPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (radius, border) = (cx.theme().radius_lg, cx.theme().border);
        v_flex()
            .key_context(CONTEXT)
            .size_full()
            .p_4()
            .gap_3()
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::toggle_enabled))
            .on_action(cx.listener(Self::start_now))
            .on_action(cx.listener(Self::stop_now))
            .on_action(cx.listener(Self::remove))
            .on_action(cx.listener(Self::clean_up))
            .on_action(cx.listener(Self::open_add))
            .on_action(cx.listener(Self::open_file_location))
            .on_action(cx.listener(Self::copy_command))
            .on_action(cx.listener(Self::refresh))
            .child(self.render_toolbar(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .rounded(radius)
                    .border_1()
                    .border_color(border)
                    .overflow_hidden()
                    .child(DataTable::new(&self.table).bordered(false)),
            )
            .child(self.render_detail_strip(cx))
    }
}
