//! The Processes page: every process in a grouped tree or flat list, with a
//! detail strip and a right-click menu for acting on them.

mod actions;
mod affinity;
pub(crate) mod confirm;
mod detail_strip;
mod details;
mod lifecycle;
pub(crate) mod rows;
mod table;

use std::time::Instant;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::table::{DataTable, TableState};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::*;
use serde::Deserialize;

use self::lifecycle::Lifecycle;
use self::rows::{Row, ViewMode};
use self::table::ProcessTable;
use crate::system::monitor::Monitor;

const CONTEXT: &str = "Processes";
/// Only while the table itself has focus, so typing in the filter is untouched.
const TABLE_CONTEXT: &str = "Processes > DataTable";
const FILTER_WIDTH: f32 = 280.;
const PAGE_ROWS: usize = 20;

actions!(
    processes,
    [
        ShowDetails,
        Terminate,
        ForceTerminate,
        EndTree,
        ForceQuitTree,
        Suspend,
        Resume,
        SetAffinity,
        CreateSample,
        OpenFileLocation,
        SearchOnline,
        CopyName,
        ToggleFollow,
        ExpandAll,
        CollapseAll,
        SelectNext,
        SelectPrevious,
        SelectFirst,
        SelectLast,
        SelectPageDown,
        SelectPageUp,
        ExpandRow,
        CollapseRow,
        FocusFilter
    ]
);

/// Sends this signal number to the selected process.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = processes, no_json)]
pub struct SendSignal(pub i32);

/// Sets the selected process's nice value.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = processes, no_json)]
pub struct SetNice(pub i32);

/// True for the flat list, false for the grouped tree.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = processes, no_json)]
pub struct SetFlatView(pub bool);

/// True to show memory as a share of total RAM.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = processes, no_json)]
pub struct ShowMemoryPercent(pub bool);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some(TABLE_CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(TABLE_CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(TABLE_CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(TABLE_CONTEXT)),
        KeyBinding::new("pagedown", SelectPageDown, Some(TABLE_CONTEXT)),
        KeyBinding::new("pageup", SelectPageUp, Some(TABLE_CONTEXT)),
        KeyBinding::new("right", ExpandRow, Some(TABLE_CONTEXT)),
        KeyBinding::new("left", CollapseRow, Some(TABLE_CONTEXT)),
        KeyBinding::new("enter", ShowDetails, Some(TABLE_CONTEXT)),
        KeyBinding::new("delete", Terminate, Some(TABLE_CONTEXT)),
        KeyBinding::new("shift-delete", ForceTerminate, Some(TABLE_CONTEXT)),
        KeyBinding::new("ctrl-f", FocusFilter, Some(CONTEXT)),
    ]);
}

pub struct ProcessesPage {
    monitor: Entity<Monitor>,
    table: Entity<TableState<ProcessTable>>,
    filter: Entity<InputState>,
    lifecycle: Lifecycle,
    active: bool,
    _subscriptions: Vec<Subscription>,
}

impl ProcessesPage {
    pub fn new(monitor: Entity<Monitor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| TableState::new(ProcessTable::new(), window, cx).row_selectable(false).col_movable(false));
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by name, user or PID"));
        let subscriptions = vec![
            cx.observe(&monitor, |this, _, cx| {
                if this.active {
                    this.refresh(cx);
                }
            }),
            cx.subscribe_in(&filter, window, |this, filter, event: &InputEvent, window, cx| match event {
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
            }),
        ];
        Self { monitor, table, filter, lifecycle: Lifecycle::default(), active: false, _subscriptions: subscriptions }
    }

    /// Called when the page is shown or hidden. Per-process details are only
    /// read while it's showing.
    pub fn set_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        self.monitor.read(cx).set_detailed(active);
        if active {
            self.lifecycle.reset();
            self.refresh(cx);
        }
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(snapshot) = self.monitor.read(cx).latest() else {
            return;
        };
        let processes = snapshot.processes.as_ref().clone();
        let total_memory = snapshot.memory.total;
        let (marks, ghosts) = self.lifecycle.update(&processes, Instant::now());
        self.table.update(cx, |state, cx| {
            let table = state.delegate_mut();
            table.set_processes(processes, ghosts, marks, total_memory);
            if let Some(pid) = table.follow {
                match table.row_of(pid) {
                    Some(row) => {
                        table.selected = Some(pid);
                        state.vertical_scroll_handle.scroll_to_item(row, ScrollStrategy::Nearest);
                    }
                    // Followed processes that exit stop being followed.
                    None if table.process(pid).is_none() => table.follow = None,
                    None => {}
                }
            }
            cx.notify();
        });
        cx.notify();
    }

    fn update_table(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut ProcessTable)) {
        self.table.update(cx, |state, cx| {
            f(state.delegate_mut());
            cx.notify();
        });
        cx.notify();
    }

    /// The selected process's PID, when it's still running.
    fn selected(&self, cx: &App) -> Option<u32> {
        let table = self.table.read(cx).delegate();
        table.selected.filter(|&pid| table.process(pid).is_some())
    }

    /// Moves the selection to a row, skipping group rows in the direction of travel.
    fn select_row(&mut self, row: usize, forward: bool, cx: &mut Context<Self>) {
        self.table.update(cx, |state, cx| {
            let table = state.delegate_mut();
            let rows = table.rows().len();
            if rows == 0 {
                return;
            }
            let mut row = row.min(rows - 1);
            while table.pid_at(row).is_none() {
                match (forward, row) {
                    (true, r) if r + 1 < rows => row += 1,
                    (false, r) if r > 0 => row -= 1,
                    _ => return,
                }
            }
            table.selected = table.pid_at(row);
            state.vertical_scroll_handle.scroll_to_item(row, ScrollStrategy::Nearest);
            cx.notify();
        });
        cx.notify();
    }

    fn selected_row(&self, cx: &App) -> Option<usize> {
        let table = self.table.read(cx).delegate();
        table.selected.and_then(|pid| table.row_of(pid))
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

    fn select_page_down(&mut self, _: &SelectPageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.step(PAGE_ROWS as isize, cx);
    }

    fn select_page_up(&mut self, _: &SelectPageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.step(-(PAGE_ROWS as isize), cx);
    }

    fn select_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        self.select_row(0, true, cx);
    }

    fn select_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        let last = self.table.read(cx).delegate().rows().len().saturating_sub(1);
        self.select_row(last, false, cx);
    }

    /// Right arrow: open the selected process's children.
    fn expand_row(&mut self, _: &ExpandRow, _: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row(cx) else {
            return;
        };
        let table = self.table.read(cx).delegate();
        if let Some(Row::Process { has_children: true, expanded: false, .. }) = table.rows().get(row) {
            let pid = table.pid_at(row);
            self.update_table(cx, |table| pid.into_iter().for_each(|pid| table.toggle_expanded(pid)));
        }
    }

    /// Left arrow: close the selected process, or jump to its parent.
    fn collapse_row(&mut self, _: &CollapseRow, _: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row(cx) else {
            return;
        };
        let table = self.table.read(cx).delegate();
        let Some(pid) = table.pid_at(row) else {
            return;
        };
        if let Some(Row::Process { expanded: true, .. }) = table.rows().get(row) {
            self.update_table(cx, |table| table.toggle_expanded(pid));
        } else if let Some(parent) = table.process(pid).map(|process| process.ppid).and_then(|ppid| table.row_of(ppid)) {
            self.select_row(parent, false, cx);
        }
    }

    fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.update(cx, |filter, cx| filter.focus(window, cx));
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let table = self.table.read(cx).delegate();
        let flat = table.mode == ViewMode::Flat;
        let live = table.processes().iter().filter(|process| process.state != 'X').count();
        h_flex()
            .gap_3()
            .items_center()
            .child(div().text_lg().font_semibold().child("Processes"))
            .child(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("{live} running")))
            .child(div().flex_1())
            .child(
                Button::new("process-view")
                    .ghost()
                    .small()
                    .label(if flat { "Flat list" } else { "Grouped tree" })
                    .icon(Icon::new(IconName::ChevronDown))
                    .tooltip("How processes are listed")
                    .dropdown_menu(move |menu, _, _| {
                        menu.menu_with_check("Grouped tree", !flat, Box::new(SetFlatView(false)))
                            .menu_with_check("Flat list", flat, Box::new(SetFlatView(true)))
                            .separator()
                            .menu_with_disabled("Expand all", Box::new(ExpandAll), flat)
                            .menu_with_disabled("Collapse all", Box::new(CollapseAll), flat)
                    }),
            )
            .child(
                div().w(px(FILTER_WIDTH)).child(
                    Input::new(&self.filter)
                        .small()
                        .cleanable(true)
                        .prefix(Icon::new(IconName::Search).small().text_color(cx.theme().muted_foreground)),
                ),
            )
    }
}

impl Render for ProcessesPage {
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
            .on_action(cx.listener(Self::expand_row))
            .on_action(cx.listener(Self::collapse_row))
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::show_details))
            .on_action(cx.listener(Self::terminate))
            .on_action(cx.listener(Self::force_terminate))
            .on_action(cx.listener(Self::end_tree))
            .on_action(cx.listener(Self::force_quit_tree))
            .on_action(cx.listener(Self::suspend))
            .on_action(cx.listener(Self::resume))
            .on_action(cx.listener(Self::send_signal))
            .on_action(cx.listener(Self::set_nice))
            .on_action(cx.listener(Self::set_affinity))
            .on_action(cx.listener(Self::create_sample))
            .on_action(cx.listener(Self::open_file_location))
            .on_action(cx.listener(Self::search_online))
            .on_action(cx.listener(Self::copy_name))
            .on_action(cx.listener(Self::toggle_follow))
            .on_action(cx.listener(Self::expand_all))
            .on_action(cx.listener(Self::collapse_all))
            .on_action(cx.listener(Self::set_flat_view))
            .on_action(cx.listener(Self::show_memory_percent))
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
