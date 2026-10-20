//! The Users page: who's signed in and what every account is running, with
//! a menu to lock or sign out a session.

mod detail_strip;
mod table;

use std::time::Duration;

use gpui_kit::component::notification::Notification;
use gpui_kit::component::table::{DataTable, TableState};
use gpui_kit::component::{ActiveTheme as _, StyledExt as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;

use self::table::{Selection, UsersTable};
use crate::system::monitor::Monitor;
use crate::system::sessions::{self, Session};
use crate::ui::processes::confirm;

const CONTEXT: &str = "Users";
/// Only while the table has focus.
const TABLE_CONTEXT: &str = "Users > DataTable";
/// Sessions change rarely, and each read runs `loginctl` twice.
const SESSION_REFRESH: Duration = Duration::from_secs(5);

actions!(users, [Lock, SignOut, ToggleProcesses, CopyName, SelectNext, SelectPrevious, SelectFirst, SelectLast, ExpandRow, CollapseRow]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some(TABLE_CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(TABLE_CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(TABLE_CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(TABLE_CONTEXT)),
        KeyBinding::new("right", ExpandRow, Some(TABLE_CONTEXT)),
        KeyBinding::new("left", CollapseRow, Some(TABLE_CONTEXT)),
    ]);
}

pub struct UsersPage {
    monitor: Entity<Monitor>,
    table: Entity<TableState<UsersTable>>,
    sessions: Vec<Session>,
    /// Re-reads sessions while the page is showing; dropping it stops the loop.
    session_task: Option<Task<()>>,
    active: bool,
    _subscription: Subscription,
}

impl UsersPage {
    pub fn new(monitor: Entity<Monitor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| TableState::new(UsersTable::new(), window, cx).row_selectable(false).col_movable(false));
        let subscription = cx.observe(&monitor, |this, _, cx| {
            if this.active {
                this.refresh(cx);
            }
        });
        Self { monitor, table, sessions: Vec::new(), session_task: None, active: false, _subscription: subscription }
    }

    pub fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        if !active {
            self.session_task = None;
            return;
        }
        self.table.read(cx).focus_handle(cx).focus(window, cx);
        self.refresh(cx);
        self.session_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let sessions = cx.background_spawn(async { sessions::read() }).await;
                let updated = this.update(cx, |this, cx| {
                    this.sessions = sessions;
                    this.refresh(cx);
                });
                if updated.is_err() {
                    break;
                }
                cx.background_executor().timer(SESSION_REFRESH).await;
            }
        }));
    }

    /// Re-adds each account's usage from the latest snapshot.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.monitor.read(cx).latest() else {
            return;
        };
        let users = sessions::usage(&snapshot.processes, &self.sessions);
        let processes = snapshot.processes.clone();
        let (total_memory, uptime) = (snapshot.memory.total, snapshot.uptime);
        self.update_table(cx, |table| table.set_users(users, &processes, total_memory, uptime));
    }

    fn update_table(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut UsersTable)) {
        self.table.update(cx, |state, cx| {
            f(state.delegate_mut());
            cx.notify();
        });
        cx.notify();
    }

    fn selected_uid(&self, cx: &App) -> Option<u32> {
        match self.table.read(cx).delegate().selected? {
            Selection::User(uid) => Some(uid),
            Selection::Process(_) => None,
        }
    }

    /// Runs `action` on each of the selected user's sessions, off the UI thread.
    fn for_each_session(&mut self, verb: &'static str, action: fn(&str) -> Result<(), String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(user) = self.selected_uid(cx).and_then(|uid| self.table.read(cx).delegate().user(uid).cloned()) else {
            return;
        };
        let ids: Vec<String> = user.sessions.iter().map(|session| session.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        cx.spawn_in(window, async move |_, cx| {
            let result = cx.background_spawn(async move { ids.iter().try_for_each(|id| action(id)) }).await;
            if let Err(err) = result {
                _ = cx.update(|window, cx| window.push_notification(Notification::error(format!("Couldn't {verb} {}: {err}", user.name)), cx));
            }
        })
        .detach();
    }

    fn lock(&mut self, _: &Lock, window: &mut Window, cx: &mut Context<Self>) {
        self.for_each_session("lock", sessions::lock, window, cx);
    }

    fn sign_out(&mut self, _: &SignOut, window: &mut Window, cx: &mut Context<Self>) {
        let Some(user) = self.selected_uid(cx).and_then(|uid| self.table.read(cx).delegate().user(uid).cloned()) else {
            return;
        };
        let count = user.sessions.len();
        if count == 0 {
            return;
        }
        // SAFETY: getuid only returns a number.
        let me = user.uid == unsafe { libc::getuid() };
        let sessions = if count == 1 { "1 session".to_string() } else { format!("{count} sessions") };
        let message = if me {
            format!("This ends {sessions}. Every app in it closes, purrtop included, and unsaved work is lost.")
        } else {
            format!("This ends {sessions}. Every app in it closes and unsaved work is lost.")
        };
        let page = cx.entity().downgrade();
        confirm::confirm(
            format!("Sign out {}?", user.name),
            message,
            "Sign out",
            move |window, cx| {
                _ = page.update(cx, |page, cx| page.for_each_session("sign out", sessions::terminate, window, cx));
            },
            window,
            cx,
        );
    }

    fn toggle_processes(&mut self, _: &ToggleProcesses, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(uid) = self.selected_uid(cx) {
            self.update_table(cx, |table| table.toggle_expanded(uid));
        }
    }

    fn copy_name(&mut self, _: &CopyName, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.selected_uid(cx).and_then(|uid| self.table.read(cx).delegate().user(uid).map(|user| user.name.clone())) else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(name.clone()));
        window.push_notification(Notification::success(format!("Copied \"{name}\"")), cx);
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
            while table.selection_at(row).is_none() {
                match (forward, row) {
                    (true, r) if r + 1 < rows => row += 1,
                    (false, r) if r > 0 => row -= 1,
                    _ => return,
                }
            }
            table.selected = table.selection_at(row);
            state.vertical_scroll_handle.scroll_to_item(row, ScrollStrategy::Nearest);
            cx.notify();
        });
        cx.notify();
    }

    fn selected_row(&self, cx: &App) -> Option<usize> {
        let table = self.table.read(cx).delegate();
        table.selected.and_then(|selection| table.row_of(selection))
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let row = match self.selected_row(cx) {
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

    /// Right arrow: show the selected user's processes.
    fn expand_row(&mut self, _: &ExpandRow, _: &mut Window, cx: &mut Context<Self>) {
        let table = self.table.read(cx).delegate();
        if let Some(uid) = self.selected_uid(cx).filter(|uid| !table.expanded.contains(uid)) {
            self.update_table(cx, |table| table.toggle_expanded(uid));
        }
    }

    /// Left arrow: hide the selected user's processes, or jump from a process to its user.
    fn collapse_row(&mut self, _: &CollapseRow, _: &mut Window, cx: &mut Context<Self>) {
        let table = self.table.read(cx).delegate();
        match table.selected {
            Some(Selection::User(uid)) if table.expanded.contains(&uid) => self.update_table(cx, |table| table.toggle_expanded(uid)),
            Some(Selection::Process(pid)) => {
                let owner = table.process(pid).and_then(|process| table.row_of(Selection::User(process.uid)));
                if let Some(row) = owner {
                    self.select_row(row, false, cx);
                }
            }
            _ => {}
        }
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let table = self.table.read(cx).delegate();
        let signed_in = table.users().iter().filter(|user| !user.sessions.is_empty()).count();
        let accounts = table.users().len();
        h_flex()
            .gap_3()
            .items_center()
            .child(div().text_lg().font_semibold().child("Users"))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{signed_in} signed in · {accounts} accounts")))
    }
}

impl Render for UsersPage {
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
            .on_action(cx.listener(Self::expand_row))
            .on_action(cx.listener(Self::collapse_row))
            .on_action(cx.listener(Self::lock))
            .on_action(cx.listener(Self::sign_out))
            .on_action(cx.listener(Self::toggle_processes))
            .on_action(cx.listener(Self::copy_name))
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
