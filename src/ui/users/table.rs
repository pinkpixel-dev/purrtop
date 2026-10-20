//! The users table: signed-in people first, then a folded group of system
//! accounts. Each user opens to show what it's running.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::*;
use crate::system::format;
use crate::system::processes::ProcessInfo;
use crate::system::sessions::UserUsage;
use crate::ui::charts::Palette;

const INDENT: f32 = 20.;
/// Usage at or above these gets its cell tinted, like the Processes page.
const HIGHLIGHT_CPU: f32 = 0.01;
const HIGHLIGHT_MEMORY: f32 = 0.02;

/// `(key, title, width, right aligned)` per column, left to right.
const COLUMNS: [(&str, &str, f32, bool); 7] = [
    ("name", "Name", 260., false),
    ("status", "Status", 96., false),
    ("session", "Session", 260., false),
    ("pid", "PID", 80., true),
    ("cpu", "CPU", 80., true),
    ("memory", "Memory", 100., true),
    ("processes", "Processes", 96., true),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    /// Signed in first, then busiest first.
    #[default]
    Usage,
    Name,
    Status,
    Cpu,
    Memory,
    Processes,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    User(usize),
    Process { pid: u32 },
    Group { count: usize, collapsed: bool },
}

/// Users and processes are tracked by ID, since rows move every second.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    User(u32),
    Process(u32),
}

pub struct UsersTable {
    users: Vec<UserUsage>,
    processes: HashMap<u32, ProcessInfo>,
    rows: Vec<Row>,
    columns: Vec<Column>,
    sort: (SortKey, bool),
    total_memory: u64,
    pub uptime: Duration,
    pub expanded: HashSet<u32>,
    /// System accounts start out folded away.
    pub system_collapsed: bool,
    pub selected: Option<Selection>,
    pub loaded: bool,
}

impl UsersTable {
    pub fn new() -> Self {
        let columns = COLUMNS
            .iter()
            .map(|&(key, title, width, right)| {
                let column = Column::new(key, title).width(width).sortable();
                if right { column.text_right() } else { column }
            })
            .collect();
        Self {
            users: Vec::new(),
            processes: HashMap::new(),
            rows: Vec::new(),
            columns,
            sort: Default::default(),
            total_memory: 0,
            uptime: Duration::ZERO,
            expanded: HashSet::new(),
            system_collapsed: true,
            selected: None,
            loaded: false,
        }
    }

    pub fn set_users(&mut self, users: Vec<UserUsage>, processes: &[ProcessInfo], total_memory: u64, uptime: Duration) {
        self.processes = processes.iter().map(|process| (process.pid, process.clone())).collect();
        self.users = users;
        self.total_memory = total_memory;
        self.uptime = uptime;
        self.loaded = true;
        self.selected = self.selected.filter(|&selection| match selection {
            Selection::User(uid) => self.user(uid).is_some(),
            Selection::Process(pid) => self.processes.contains_key(&pid),
        });
        self.rebuild();
    }

    pub fn users(&self) -> &[UserUsage] {
        &self.users
    }

    pub fn user(&self, uid: u32) -> Option<&UserUsage> {
        self.users.iter().find(|user| user.uid == uid)
    }

    pub fn process(&self, pid: u32) -> Option<&ProcessInfo> {
        self.processes.get(&pid)
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn selection_at(&self, row: usize) -> Option<Selection> {
        match self.rows.get(row)? {
            Row::User(ix) => Some(Selection::User(self.users[*ix].uid)),
            Row::Process { pid } => Some(Selection::Process(*pid)),
            Row::Group { .. } => None,
        }
    }

    pub fn row_of(&self, selection: Selection) -> Option<usize> {
        self.rows.iter().position(|&row| match (row, selection) {
            (Row::User(ix), Selection::User(uid)) => self.users[ix].uid == uid,
            (Row::Process { pid }, Selection::Process(selected)) => pid == selected,
            _ => false,
        })
    }

    pub fn toggle_expanded(&mut self, uid: u32) {
        if !self.expanded.remove(&uid) {
            self.expanded.insert(uid);
        }
        self.rebuild();
    }

    pub fn toggle_system(&mut self) {
        self.system_collapsed = !self.system_collapsed;
        self.rebuild();
    }

    pub fn rebuild(&mut self) {
        let (key, descending) = self.sort;
        let mut order: Vec<usize> = (0..self.users.len()).collect();
        if key != SortKey::Usage {
            order.sort_by(|&a, &b| {
                let (a, b) = (&self.users[a], &self.users[b]);
                let ordering = match key {
                    SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    SortKey::Status => a.status().cmp(&b.status()),
                    SortKey::Memory => a.memory.cmp(&b.memory),
                    SortKey::Processes => a.processes.len().cmp(&b.processes.len()),
                    _ => a.cpu.total_cmp(&b.cpu),
                };
                if descending { ordering.reverse() } else { ordering }
            });
        }
        let (people, system): (Vec<usize>, Vec<usize>) = order.into_iter().partition(|&ix| self.users[ix].is_person());
        let mut rows = Vec::new();
        for ix in people {
            self.push_user(ix, &mut rows);
        }
        if !system.is_empty() {
            rows.push(Row::Group { count: system.len(), collapsed: self.system_collapsed });
            if !self.system_collapsed {
                for ix in system {
                    self.push_user(ix, &mut rows);
                }
            }
        }
        self.rows = rows;
    }

    fn push_user(&self, ix: usize, rows: &mut Vec<Row>) {
        rows.push(Row::User(ix));
        let user = &self.users[ix];
        if !self.expanded.contains(&user.uid) {
            return;
        }
        let mut pids = user.processes.clone();
        // Processes come busiest first; only re-sort for the columns that apply to them.
        let (key, descending) = self.sort;
        if matches!(key, SortKey::Name | SortKey::Memory) {
            pids.sort_by(|a, b| {
                let (a, b) = (&self.processes[a], &self.processes[b]);
                let ordering = match key {
                    SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                    _ => a.memory.cmp(&b.memory),
                };
                if descending { ordering.reverse() } else { ordering }
            });
        }
        rows.extend(pids.into_iter().filter(|pid| self.processes.contains_key(pid)).map(|pid| Row::Process { pid }));
    }

    fn render_name(&self, row_ix: usize, user: &UserUsage, cx: &mut Context<TableState<Self>>) -> AnyElement {
        let theme = cx.theme();
        let (uid, open) = (user.uid, self.expanded.contains(&user.uid));
        let toggle = div()
            .id(("user-toggle", row_ix))
            .size_4()
            .flex_shrink_0()
            .when(!user.processes.is_empty(), |this| {
                this.cursor_pointer().child(chevron(open).text_color(theme.muted_foreground)).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |table, _, _, cx| {
                        cx.stop_propagation();
                        table.delegate_mut().toggle_expanded(uid);
                        cx.notify();
                    }),
                )
            });
        let icon = if user.sessions.is_empty() { CatalogIcon::Settings } else { CatalogIcon::User };
        h_flex()
            .min_w_0()
            .gap_1()
            .child(toggle)
            .child(Icon::new(icon).xsmall().text_color(if user.sessions.is_empty() { theme.muted_foreground } else { theme.primary }))
            .child(div().min_w_0().truncate().when(!user.sessions.is_empty(), |this| this.font_semibold()).child(user.name.clone()))
            .into_any_element()
    }

    fn render_usage(&self, cpu: f32, memory: u64, column: &str, cx: &App) -> AnyElement {
        let palette = Palette::new(cx);
        let tinted = |text: String, color: Hsla, on: bool| {
            div().w_full().px_1().text_right().when(on, |this| this.bg(color.opacity(0.18)).text_color(color)).child(text).into_any_element()
        };
        if column == "cpu" {
            return tinted(format::percent(cpu), palette.cpu, cpu >= HIGHLIGHT_CPU);
        }
        let share = if self.total_memory == 0 { 0. } else { memory as f32 / self.total_memory as f32 };
        tinted(format::bytes(memory), palette.memory, share >= HIGHLIGHT_MEMORY)
    }
}

fn chevron(open: bool) -> Icon {
    Icon::new(if open { CatalogIcon::ChevronDown } else { CatalogIcon::ChevronRight }).xsmall()
}

impl TableDelegate for UsersTable {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn loading(&self, _: &App) -> bool {
        !self.loaded
    }

    fn perform_sort(&mut self, col_ix: usize, sort: ColumnSort, _: &mut Window, _: &mut Context<TableState<Self>>) {
        let key = match self.columns[col_ix].key.as_ref() {
            "name" => SortKey::Name,
            "status" => SortKey::Status,
            "cpu" => SortKey::Cpu,
            "memory" => SortKey::Memory,
            "processes" => SortKey::Processes,
            _ => SortKey::Usage,
        };
        self.sort = match sort {
            ColumnSort::Ascending => (key, false),
            ColumnSort::Descending => (key, true),
            _ => Default::default(),
        };
        self.rebuild();
    }

    fn render_tr(&mut self, row_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> Stateful<Div> {
        let theme = cx.theme();
        let tr = div().id(("users-row", row_ix));
        match self.selection_at(row_ix) {
            None => tr.cursor_pointer().on_click(cx.listener(|table, _, _, cx| {
                table.delegate_mut().toggle_system();
                cx.notify();
            })),
            Some(selection) => tr.when(self.selected == Some(selection), |this| this.bg(theme.list_active)).on_click(cx.listener(
                move |table, event: &ClickEvent, _, cx| {
                    let table = table.delegate_mut();
                    table.selected = Some(selection);
                    // Double-clicking a user opens or closes its processes.
                    if let (Selection::User(uid), 2) = (selection, event.click_count()) {
                        table.toggle_expanded(uid);
                    }
                    cx.notify();
                },
            )),
        }
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let theme = cx.theme();
        let column = self.columns[col_ix].key.clone();
        let muted = |text: &str| div().text_color(theme.muted_foreground).child(text.to_string()).into_any_element();
        match self.rows.get(row_ix).copied() {
            Some(Row::Group { count, collapsed }) if column.as_ref() == "name" => h_flex()
                .gap_1()
                .child(chevron(!collapsed).text_color(theme.muted_foreground))
                .child(div().font_semibold().child(format!("System accounts ({count})")))
                .into_any_element(),
            Some(Row::User(ix)) => {
                let user = self.users[ix].clone();
                match column.as_ref() {
                    "name" => self.render_name(row_ix, &user, cx),
                    "status" => match user.status() {
                        Some(status) => {
                            let color = if status == "Active" { theme.success } else { theme.foreground };
                            div().text_color(color).child(status).into_any_element()
                        }
                        None => muted("—"),
                    },
                    "session" => match user.sessions.as_slice() {
                        [] => muted("—"),
                        [session] => div().min_w_0().truncate().child(session.label()).into_any_element(),
                        [session, rest @ ..] => div().min_w_0().truncate().child(format!("{} +{}", session.label(), rest.len())).into_any_element(),
                    },
                    "cpu" | "memory" => self.render_usage(user.cpu, user.memory, &column, cx),
                    "processes" => user.processes.len().to_string().into_any_element(),
                    _ => div().into_any_element(),
                }
            }
            Some(Row::Process { pid }) => {
                let Some(process) = self.processes.get(&pid).cloned() else {
                    return div().into_any_element();
                };
                match column.as_ref() {
                    "name" => div().min_w_0().pl(px(INDENT)).truncate().child(process.name.clone()).into_any_element(),
                    "status" => div().text_color(theme.muted_foreground).child(process.state_label()).into_any_element(),
                    "pid" => process.pid.to_string().into_any_element(),
                    "cpu" | "memory" => self.render_usage(process.cpu, process.memory, &column, cx),
                    _ => div().into_any_element(),
                }
            }
            _ => div().into_any_element(),
        }
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, _: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(Selection::User(uid)) = self.selection_at(row_ix) else {
            return menu;
        };
        let Some(user) = self.user(uid) else {
            return menu;
        };
        let signed_in = !user.sessions.is_empty();
        let all_locked = user.sessions.iter().all(|session| session.locked);
        let no_processes = user.processes.is_empty();
        let open = self.expanded.contains(&uid);
        // Right-click selects, so every action works on the selected user.
        self.selected = Some(Selection::User(uid));
        cx.notify();

        menu.menu_with_disabled("Lock", Box::new(Lock), !signed_in || all_locked)
            .menu_with_disabled("Sign out…", Box::new(SignOut), !signed_in)
            .separator()
            .menu_with_disabled(if open { "Hide processes" } else { "Show processes" }, Box::new(ToggleProcesses), no_processes)
            .menu("Copy name", Box::new(CopyName))
    }
}
