//! The Ports page: what's listening on which port, who holds it, and a way
//! to end or kill it.

mod actions;
mod detail_strip;
mod table;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::table::{DataTable, TableState};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use self::table::{Port, PortsTable};
use crate::system::monitor::Monitor;
use crate::system::ports::{self, Socket};
use crate::system::users::Users;

const CONTEXT: &str = "Ports";
/// Only while the table itself has focus, so typing in the filter is untouched.
const TABLE_CONTEXT: &str = "Ports > DataTable";
const FILTER_WIDTH: f32 = 280.;
const PAGE_ROWS: usize = 20;
/// Each read walks every fd we can see, so a little slower than the monitor.
const REFRESH: Duration = Duration::from_secs(2);

actions!(
    ports,
    [EndProcess, KillProcess, FindOwners, OpenInProcesses, CopyAddress, SelectNext, SelectPrevious, SelectFirst, SelectLast, SelectPageDown, SelectPageUp, FocusFilter]
);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some(TABLE_CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(TABLE_CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(TABLE_CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(TABLE_CONTEXT)),
        KeyBinding::new("pagedown", SelectPageDown, Some(TABLE_CONTEXT)),
        KeyBinding::new("pageup", SelectPageUp, Some(TABLE_CONTEXT)),
        KeyBinding::new("delete", EndProcess, Some(TABLE_CONTEXT)),
        KeyBinding::new("shift-delete", KillProcess, Some(TABLE_CONTEXT)),
        KeyBinding::new("ctrl-f", FocusFilter, Some(CONTEXT)),
    ]);
}

pub struct PortsPage {
    monitor: Entity<Monitor>,
    table: Entity<TableState<PortsTable>>,
    filter: Entity<InputState>,
    users: Users,
    /// Owners found by the admin lookup, by socket inode. Inodes aren't
    /// reused while a socket lives, so these stay right until it closes.
    admin_owners: HashMap<u64, Vec<u32>>,
    /// Re-reads the ports while the page is showing; dropping it stops the loop.
    refresh_task: Option<Task<()>>,
    looking_up: bool,
    active: bool,
    _subscription: Subscription,
}

impl PortsPage {
    pub fn new(monitor: Entity<Monitor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| TableState::new(PortsTable::new(), window, cx).row_selectable(false).col_movable(false));
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by port, address, process or user"));
        let subscription = cx.subscribe_in(&filter, window, |this, filter, event: &InputEvent, window, cx| match event {
            InputEvent::Change => {
                let text = filter.read(cx).value().to_string();
                this.update_table(cx, |table| {
                    table.filter = text;
                    table.rebuild();
                });
            }
            // Enter hands the keyboard to the table, ready for arrow keys.
            InputEvent::PressEnter { .. } => this.table.read(cx).focus_handle(cx).focus(window, cx),
            _ => {}
        });
        Self {
            monitor,
            table,
            filter,
            users: Users::load(),
            admin_owners: HashMap::new(),
            refresh_task: None,
            looking_up: false,
            active: false,
            _subscription: subscription,
        }
    }

    pub fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        if !active {
            self.refresh_task = None;
            return;
        }
        self.table.read(cx).focus_handle(cx).focus(window, cx);
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            loop {
                if this.update(cx, |this, cx| this.refresh(cx)).is_err() {
                    break;
                }
                cx.background_executor().timer(REFRESH).await;
            }
        }));
    }

    /// Reads the sockets and their owners off the UI thread, then fills in
    /// names from the monitor's latest process snapshot.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let known: HashSet<u64> = self.admin_owners.keys().copied().collect();
        cx.spawn(async move |this, cx| {
            let (sockets, owners) = cx
                .background_spawn(async move {
                    let sockets = ports::read();
                    let wanted = sockets.iter().map(|socket| socket.inode).filter(|inode| !known.contains(inode)).collect();
                    (sockets, ports::owners(&wanted))
                })
                .await;
            _ = this.update(cx, |this, cx| this.set_ports(sockets, owners, cx));
        })
        .detach();
    }

    fn set_ports(&mut self, sockets: Vec<Socket>, owners: HashMap<u64, Vec<u32>>, cx: &mut Context<Self>) {
        let open: HashSet<u64> = sockets.iter().map(|socket| socket.inode).collect();
        self.admin_owners.retain(|inode, _| open.contains(inode));
        let processes = self.monitor.read(cx).latest().map(|snapshot| snapshot.processes.clone()).unwrap_or_default();
        let ports = sockets
            .into_iter()
            .map(|socket| {
                let pids = owners.get(&socket.inode).or_else(|| self.admin_owners.get(&socket.inode)).cloned().unwrap_or_default();
                let pid = ports::main_pid(&pids);
                let process = pid.and_then(|pid| processes.iter().find(|process| process.pid == pid));
                Port {
                    user: process.map_or_else(|| self.users.name(socket.uid), |process| process.user.clone()),
                    name: process.map(|process| process.name.clone()),
                    command: process.map(|process| process.command.clone()),
                    socket,
                    pids,
                    pid,
                }
            })
            .collect();
        self.update_table(cx, |table| table.set_ports(ports));
    }

    fn update_table(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut PortsTable)) {
        self.table.update(cx, |state, cx| {
            f(state.delegate_mut());
            cx.notify();
        });
        cx.notify();
    }

    fn select_row(&mut self, row: usize, cx: &mut Context<Self>) {
        self.table.update(cx, |state, cx| {
            let table = state.delegate_mut();
            let rows = table.row_count();
            if rows == 0 {
                return;
            }
            let row = row.min(rows - 1);
            table.selected = table.inode_at(row);
            state.vertical_scroll_handle.scroll_to_item(row, ScrollStrategy::Nearest);
            cx.notify();
        });
        cx.notify();
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let table = self.table.read(cx).delegate();
        let row = match table.selected.and_then(|inode| table.row_of(inode)) {
            Some(row) => row.saturating_add_signed(delta),
            None => 0,
        };
        self.select_row(row, cx);
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.step(1, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-1, cx);
    }

    fn select_page_down(&mut self, _: &SelectPageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.step(PAGE_ROWS as isize, cx);
    }

    fn select_page_up(&mut self, _: &SelectPageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-(PAGE_ROWS as isize), cx);
    }

    fn select_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        self.select_row(0, cx);
    }

    fn select_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        let last = self.table.read(cx).delegate().row_count().saturating_sub(1);
        self.select_row(last, cx);
    }

    fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.update(cx, |filter, cx| filter.focus(window, cx));
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let ports = self.table.read(cx).delegate().ports();
        let hidden = ports.iter().filter(|port| port.pid.is_none()).count();
        let theme = cx.theme();
        h_flex()
            .gap_3()
            .items_center()
            .child(div().text_lg().font_semibold().child("Ports"))
            .child(div().text_xs().text_color(theme.muted_foreground).child(format!("{} listening", ports.len())))
            .child(div().flex_1())
            .when(hidden > 0, |this| {
                this.child(
                    Button::new("ports-find-owners")
                        .ghost()
                        .small()
                        .label("Find hidden owners")
                        .icon(Icon::new(IconName::Search))
                        .loading(self.looking_up)
                        .tooltip("Look up processes that need admin rights to see")
                        .on_click(cx.listener(|this, _, window, cx| this.find_owners(&FindOwners, window, cx))),
                )
            })
            .child(
                div().w(px(FILTER_WIDTH)).child(
                    Input::new(&self.filter)
                        .small()
                        .cleanable(true)
                        .prefix(Icon::new(IconName::Search).small().text_color(theme.muted_foreground)),
                ),
            )
    }
}

impl Render for PortsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (radius, border) = (cx.theme().radius_lg, cx.theme().border);
        v_flex()
            .key_context(CONTEXT)
            .size_full()
            .p_4()
            .gap_3()
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_page_down))
            .on_action(cx.listener(Self::select_page_up))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::end_process))
            .on_action(cx.listener(Self::kill_process))
            .on_action(cx.listener(Self::find_owners))
            .on_action(cx.listener(Self::open_in_processes))
            .on_action(cx.listener(Self::copy_address))
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
