//! The process table: columns, cells, row highlights and the right-click menu.

use std::collections::{HashMap, HashSet};

use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::rows::{Mark, Row, RowOptions, Sort, SortKey, ViewMode, build_rows};
use super::*;
use crate::system::control::SIGNALS;
use crate::system::format;
use crate::system::processes::{Kind, ProcessInfo};
use crate::ui::charts::Palette;

const INDENT: f32 = 16.;
/// Usage at or above these gets its cell tinted.
const HIGHLIGHT_CPU: f32 = 0.01;
const HIGHLIGHT_MEMORY: f32 = 0.02;

/// Nice presets for the menu: value and label.
const NICE_LEVELS: [(i32, &str); 7] = [
    (-20, "Highest (-20)"),
    (-10, "High (-10)"),
    (-5, "Above normal (-5)"),
    (0, "Normal (0)"),
    (5, "Below normal (5)"),
    (10, "Low (10)"),
    (19, "Lowest (19)"),
];

/// `(key, title, width, right aligned)` per column, left to right.
const COLUMNS: [(&str, &str, f32, bool); 13] = [
    ("name", "Name", 240., false),
    ("pid", "PID", 80., true),
    ("status", "Status", 88., false),
    ("user", "User", 110., false),
    ("cpu", "CPU", 72., true),
    ("memory", "Memory", 92., true),
    ("disk_read", "Disk read", 100., true),
    ("disk_write", "Disk write", 100., true),
    ("threads", "Threads", 76., true),
    ("virtual", "Virtual", 92., true),
    ("files", "FDs", 64., true),
    ("nice", "Nice", 56., true),
    ("command", "Command", 560., false),
];

fn sort_key(column: &str) -> Option<SortKey> {
    Some(match column {
        "name" => SortKey::Name,
        "pid" => SortKey::Pid,
        "status" => SortKey::Status,
        "user" => SortKey::User,
        "cpu" => SortKey::Cpu,
        "memory" => SortKey::Memory,
        "disk_read" => SortKey::DiskRead,
        "disk_write" => SortKey::DiskWrite,
        "threads" => SortKey::Threads,
        "virtual" => SortKey::Virtual,
        "files" => SortKey::Files,
        "nice" => SortKey::Nice,
        "command" => SortKey::Command,
        _ => return None,
    })
}

pub struct ProcessTable {
    /// Live processes followed by recently exited ones.
    processes: Vec<ProcessInfo>,
    rows: Vec<Row>,
    columns: Vec<Column>,
    marks: HashMap<u32, Mark>,
    total_memory: u64,
    pub sort: Sort,
    pub mode: ViewMode,
    pub filter: String,
    pub expanded: HashSet<u32>,
    pub collapsed_groups: HashSet<Kind>,
    /// Tracked by PID rather than row, since rows move every second.
    pub selected: Option<u32>,
    /// A process kept selected and in view while it moves around.
    pub follow: Option<u32>,
    /// Show memory as a share of total RAM instead of bytes.
    pub memory_percent: bool,
}

impl ProcessTable {
    pub fn new() -> Self {
        let columns = COLUMNS
            .iter()
            .map(|&(key, title, width, right)| {
                let column = Column::new(key, title).width(width).sortable();
                if right { column.text_right() } else { column }
            })
            .collect();
        Self {
            processes: Vec::new(),
            rows: Vec::new(),
            columns,
            marks: HashMap::new(),
            total_memory: 0,
            sort: Sort::default(),
            mode: ViewMode::default(),
            filter: String::new(),
            expanded: HashSet::new(),
            collapsed_groups: HashSet::new(),
            selected: None,
            follow: None,
            memory_percent: false,
        }
    }

    /// Takes a new reading. Exited processes go after the live ones.
    pub fn set_processes(&mut self, mut processes: Vec<ProcessInfo>, ghosts: Vec<ProcessInfo>, marks: HashMap<u32, Mark>, total_memory: u64) {
        processes.extend(ghosts);
        self.processes = processes;
        self.marks = marks;
        self.total_memory = total_memory;
        self.rebuild();
    }

    pub fn rebuild(&mut self) {
        self.rows = build_rows(
            &self.processes,
            &RowOptions {
                filter: &self.filter,
                sort: self.sort,
                mode: self.mode,
                expanded: &self.expanded,
                collapsed_groups: &self.collapsed_groups,
                marks: &self.marks,
            },
        );
    }

    pub fn processes(&self) -> &[ProcessInfo] {
        &self.processes
    }

    pub fn process(&self, pid: u32) -> Option<&ProcessInfo> {
        self.processes.iter().find(|process| process.pid == pid && process.state != 'X')
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The row showing `pid`, if it's visible.
    pub fn row_of(&self, pid: u32) -> Option<usize> {
        self.rows.iter().position(|row| row.process().is_some_and(|ix| self.processes[ix].pid == pid))
    }

    pub fn pid_at(&self, row: usize) -> Option<u32> {
        self.rows.get(row)?.process().map(|ix| self.processes[ix].pid)
    }

    /// Expands every process that has children.
    pub fn expand_all(&mut self) {
        let parents: HashSet<u32> = self.processes.iter().map(|process| process.ppid).collect();
        self.expanded = self.processes.iter().map(|process| process.pid).filter(|pid| parents.contains(pid)).collect();
        self.rebuild();
    }

    pub fn toggle_expanded(&mut self, pid: u32) {
        if !self.expanded.remove(&pid) {
            self.expanded.insert(pid);
        }
        self.rebuild();
    }

    pub fn toggle_group(&mut self, kind: Kind) {
        if !self.collapsed_groups.remove(&kind) {
            self.collapsed_groups.insert(kind);
        }
        self.rebuild();
    }

    fn render_name(&self, row: &Row, cx: &mut Context<TableState<Self>>) -> AnyElement {
        let theme = cx.theme();
        match *row {
            Row::Group { kind, count, collapsed } => {
                let label = match kind {
                    Kind::Application => "Applications",
                    Kind::Background => "Background processes",
                };
                h_flex()
                    .gap_1()
                    .child(chevron(!collapsed).text_color(theme.muted_foreground))
                    .child(div().font_semibold().child(format!("{label} ({count})")))
                    .into_any_element()
            }
            Row::Process { ix, depth, has_children, expanded, .. } => {
                let process = &self.processes[ix];
                let pid = process.pid;
                let toggle = div()
                    .id(SharedString::from(format!("toggle-{pid}")))
                    .size_4()
                    .flex_shrink_0()
                    .when(has_children, |this| {
                        this.cursor_pointer()
                            .child(chevron(expanded).text_color(theme.muted_foreground))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |table, _, _, cx| {
                                    cx.stop_propagation();
                                    table.delegate_mut().toggle_expanded(pid);
                                    cx.notify();
                                }),
                            )
                    });
                h_flex()
                    .min_w_0()
                    .gap_1()
                    .pl(px(INDENT * depth as f32))
                    .child(toggle)
                    .child(div().min_w_0().truncate().child(process.name.clone()))
                    .into_any_element()
            }
        }
    }

    fn render_value(&self, process: &ProcessInfo, column: &str, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let palette = Palette::new(cx);
        let muted = |text: &str| div().text_color(theme.muted_foreground).child(text.to_string()).into_any_element();
        let tinted = |text: String, color: Hsla, on: bool| {
            div()
                .w_full()
                .px_1()
                .text_right()
                .when(on, |this| this.bg(color.opacity(0.18)).text_color(color))
                .child(text)
                .into_any_element()
        };
        let exited = process.state == 'X';
        match column {
            "pid" => process.pid.to_string().into_any_element(),
            "status" => process.state_label().into_any_element(),
            "user" => process.user.clone().into_any_element(),
            "cpu" if exited => muted("—"),
            "cpu" => tinted(format::percent(process.cpu), palette.cpu, process.cpu >= HIGHLIGHT_CPU),
            "memory" => {
                let share = if self.total_memory == 0 { 0. } else { process.memory as f32 / self.total_memory as f32 };
                let text = if self.memory_percent { format::percent(share) } else { format::bytes(process.memory) };
                tinted(text, palette.memory, share >= HIGHLIGHT_MEMORY)
            }
            "disk_read" => process.disk_read.map_or_else(|| muted("—"), |rate| format::rate(rate).into_any_element()),
            "disk_write" => process.disk_write.map_or_else(|| muted("—"), |rate| format::rate(rate).into_any_element()),
            "threads" => process.threads.to_string().into_any_element(),
            "virtual" => format::bytes(process.virtual_memory).into_any_element(),
            "files" => process.open_files.map_or_else(|| muted("—"), |count| count.to_string().into_any_element()),
            "nice" => process.nice.to_string().into_any_element(),
            "command" => div().min_w_0().truncate().child(process.command.clone()).into_any_element(),
            _ => div().into_any_element(),
        }
    }
}

fn chevron(open: bool) -> Icon {
    Icon::new(if open { CatalogIcon::ChevronDown } else { CatalogIcon::ChevronRight }).xsmall()
}

impl TableDelegate for ProcessTable {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    fn perform_sort(&mut self, col_ix: usize, sort: ColumnSort, _: &mut Window, _: &mut Context<TableState<Self>>) {
        self.sort = match (sort_key(&self.columns[col_ix].key), sort) {
            (Some(key), ColumnSort::Ascending) => Sort { key, descending: false },
            (Some(key), ColumnSort::Descending) => Sort { key, descending: true },
            _ => Sort::default(),
        };
        self.rebuild();
    }

    fn render_tr(&mut self, row_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> Stateful<Div> {
        let theme = cx.theme();
        let tr = div().id(("row", row_ix));
        let Some(row) = self.rows.get(row_ix).cloned() else {
            return tr;
        };
        match row {
            Row::Group { kind, .. } => tr.cursor_pointer().on_click(cx.listener(move |table, _, _, cx| {
                table.delegate_mut().toggle_group(kind);
                cx.notify();
            })),
            Row::Process { ix, mark, .. } => {
                let pid = self.processes[ix].pid;
                let selected = self.selected == Some(pid);
                tr.when(mark == Mark::New, |this| this.bg(theme.success.opacity(0.16)))
                    .when(mark == Mark::Exited, |this| this.bg(theme.danger.opacity(0.2)))
                    .when(selected, |this| this.bg(theme.list_active))
                    .on_click(cx.listener(move |table, event: &ClickEvent, window, cx| {
                        table.delegate_mut().selected = Some(pid);
                        cx.notify();
                        if event.click_count() == 2 {
                            window.dispatch_action(Box::new(ShowDetails), cx);
                        }
                    }))
            }
        }
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let Some(row) = self.rows.get(row_ix).cloned() else {
            return div().into_any_element();
        };
        let column = self.columns[col_ix].key.clone();
        if column.as_ref() == "name" {
            return self.render_name(&row, cx);
        }
        match row {
            Row::Group { .. } => div().into_any_element(),
            Row::Process { ix, .. } => self.render_value(&self.processes[ix].clone(), &column, cx),
        }
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, window: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(process) = self.rows.get(row_ix).and_then(Row::process).map(|ix| self.processes[ix].clone()) else {
            return menu;
        };
        if process.state == 'X' {
            return menu.label("This process has exited");
        }
        // Right-click selects, so every action works on the selected process.
        self.selected = Some(process.pid);
        cx.notify();

        let nice = process.nice;
        let signals = PopupMenu::build(window, cx, |mut menu, _, _| {
            for (signal, label) in SIGNALS {
                menu = menu.menu(label, Box::new(SendSignal(signal)));
            }
            menu
        });
        let priorities = PopupMenu::build(window, cx, move |mut menu, _, _| {
            for (value, label) in NICE_LEVELS {
                menu = menu.menu_with_check(label, value == nice, Box::new(SetNice(value)));
            }
            menu
        });
        let flat = self.mode == ViewMode::Flat;
        let view = PopupMenu::build(window, cx, move |menu, _, _| {
            menu.menu_with_check("Grouped tree", !flat, Box::new(SetFlatView(false)))
                .menu_with_check("Flat list", flat, Box::new(SetFlatView(true)))
                .separator()
                .menu_with_disabled("Expand all", Box::new(ExpandAll), flat)
                .menu_with_disabled("Collapse all", Box::new(CollapseAll), flat)
        });
        let percent = self.memory_percent;
        let values = PopupMenu::build(window, cx, move |menu, _, _| {
            menu.menu_with_check("Memory in bytes", !percent, Box::new(ShowMemoryPercent(false)))
                .menu_with_check("Memory as percent", percent, Box::new(ShowMemoryPercent(true)))
        });

        menu.menu("Linux process details…", Box::new(ShowDetails))
            .separator()
            .menu("Terminate (SIGTERM)", Box::new(Terminate))
            .menu("Force terminate (SIGKILL)", Box::new(ForceTerminate))
            .menu("End process tree", Box::new(EndTree))
            .menu("Force quit process tree", Box::new(ForceQuitTree))
            .separator()
            .menu_with_disabled("Suspend (SIGSTOP)", Box::new(Suspend), process.state == 'T')
            .menu_with_disabled("Resume (SIGCONT)", Box::new(Resume), process.state != 'T')
            .item(PopupMenuItem::submenu("Send signal", signals))
            .item(PopupMenuItem::submenu("Nice priority", priorities))
            .menu("Set affinity…", Box::new(SetAffinity))
            .separator()
            .menu("Create process sample", Box::new(CreateSample))
            .menu_with_disabled("Open file location", Box::new(OpenFileLocation), process.exe.is_none())
            .menu("Search online", Box::new(SearchOnline))
            .menu("Copy process name", Box::new(CopyName))
            .separator()
            .item(PopupMenuItem::submenu("View", view))
            .menu_with_check("Follow process", self.follow == Some(process.pid), Box::new(ToggleFollow))
            .item(PopupMenuItem::submenu("Resource values", values))
    }
}
