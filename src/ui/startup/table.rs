//! The startup table: entries autostart starts here, then a group for the
//! rest (other desktops, or started another way), with an on/off switch per row and the right-click menu.

use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::*;
use crate::system::autostart::{Entry, Status};

/// `(key, title, width)` per column, left to right.
const COLUMNS: [(&str, &str, f32); 5] = [
    ("name", "Name", 260.),
    ("startup", "At login", 96.),
    ("status", "Status", 120.),
    ("source", "Source", 140.),
    ("command", "Command", 560.),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    #[default]
    Name,
    Startup,
    Status,
    Source,
    Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Group { count: usize, collapsed: bool },
    Entry(usize),
}

pub struct StartupTable {
    page: WeakEntity<StartupPage>,
    entries: Vec<Entry>,
    rows: Vec<Row>,
    columns: Vec<Column>,
    sort: (SortKey, bool),
    /// Entries autostart doesn't start here start out folded away.
    pub elsewhere_collapsed: bool,
    /// Tracked by file name, since a reload can reorder the rows.
    pub selected: Option<String>,
    pub loaded: bool,
}

impl StartupTable {
    pub fn new(page: WeakEntity<StartupPage>) -> Self {
        let columns = COLUMNS.iter().map(|&(key, title, width)| Column::new(key, title).width(width).sortable()).collect();
        Self { page, entries: Vec::new(), rows: Vec::new(), columns, sort: Default::default(), elsewhere_collapsed: true, selected: None, loaded: false }
    }

    pub fn set_entries(&mut self, entries: Vec<Entry>) {
        self.entries = entries;
        self.loaded = true;
        if self.selected.as_ref().is_some_and(|id| self.entry(id).is_none()) {
            self.selected = None;
        }
        self.rebuild();
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn entry(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn id_at(&self, row: usize) -> Option<String> {
        match self.rows.get(row)? {
            Row::Entry(ix) => Some(self.entries[*ix].id.clone()),
            Row::Group { .. } => None,
        }
    }

    pub fn row_of(&self, id: &str) -> Option<usize> {
        self.rows.iter().position(|row| matches!(row, Row::Entry(ix) if self.entries[*ix].id == id))
    }

    pub fn toggle_elsewhere(&mut self) {
        self.elsewhere_collapsed = !self.elsewhere_collapsed;
        self.rebuild();
    }

    fn rebuild(&mut self) {
        let (key, descending) = self.sort;
        let mut order: Vec<usize> = (0..self.entries.len()).collect();
        order.sort_by(|&a, &b| {
            let (a, b) = (&self.entries[a], &self.entries[b]);
            let ordering = match key {
                SortKey::Name => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
                SortKey::Startup => b.enabled.cmp(&a.enabled),
                SortKey::Status => a.status.label().cmp(b.status.label()),
                SortKey::Source => a.source.label().cmp(b.source.label()),
                SortKey::Command => a.exec.cmp(&b.exec),
            };
            if descending { ordering.reverse() } else { ordering }
        });
        let (here, elsewhere): (Vec<usize>, Vec<usize>) = order.into_iter().partition(|&ix| self.entries[ix].elsewhere.is_none());
        let mut rows: Vec<Row> = here.into_iter().map(Row::Entry).collect();
        if !elsewhere.is_empty() {
            rows.push(Row::Group { count: elsewhere.len(), collapsed: self.elsewhere_collapsed });
            if !self.elsewhere_collapsed {
                rows.extend(elsewhere.into_iter().map(Row::Entry));
            }
        }
        self.rows = rows;
    }

    fn render_status(entry: &Entry, cx: &App) -> AnyElement {
        let theme = cx.theme();
        // Disabled entries and ones autostart skips never start, so their unit state says little.
        if entry.status != Status::FileMissing && !entry.starts_here() && !entry.can_stop() {
            return div().text_color(theme.muted_foreground).child("—").into_any_element();
        }
        let color = match entry.status {
            Status::Running => theme.success,
            Status::Failed => theme.danger,
            Status::ProgramMissing | Status::FileMissing => theme.warning,
            _ => theme.muted_foreground,
        };
        div().text_color(color).child(entry.status.label()).into_any_element()
    }

    fn render_switch(&self, entry: &Entry) -> AnyElement {
        let (page, id, name) = (self.page.clone(), entry.id.clone(), entry.name.clone());
        Switch::new(SharedString::from(format!("startup-{}", entry.id)))
            .small()
            .checked(entry.enabled)
            .disabled(entry.status == Status::FileMissing)
            .tooltip(if entry.enabled { format!("Don't start {name} at login") } else { format!("Start {name} at login") })
            .on_click(move |checked, window, cx| {
                _ = page.update(cx, |page, cx| page.set_enabled(&id, *checked, window, cx));
            })
            .into_any_element()
    }
}

impl TableDelegate for StartupTable {
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
            "startup" => SortKey::Startup,
            "status" => SortKey::Status,
            "source" => SortKey::Source,
            "command" => SortKey::Command,
            _ => SortKey::Name,
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
        let tr = div().id(("startup-row", row_ix));
        match self.rows.get(row_ix).copied() {
            Some(Row::Group { .. }) => tr.cursor_pointer().on_click(cx.listener(|table, _, _, cx| {
                table.delegate_mut().toggle_elsewhere();
                cx.notify();
            })),
            Some(Row::Entry(ix)) => {
                let entry = &self.entries[ix];
                let id = entry.id.clone();
                let selected = self.selected.as_deref() == Some(id.as_str());
                tr.when(entry.elsewhere.is_some(), |this| this.text_color(theme.muted_foreground))
                    .when(selected, |this| this.bg(theme.list_active))
                    .on_click(cx.listener(move |table, _, _, cx| {
                        table.delegate_mut().selected = Some(id.clone());
                        cx.notify();
                    }))
            }
            None => tr,
        }
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let theme = cx.theme();
        let column = self.columns[col_ix].key.clone();
        match self.rows.get(row_ix).copied() {
            Some(Row::Group { count, collapsed, .. }) if column.as_ref() == "name" => h_flex()
                .gap_1()
                .child(Icon::new(if collapsed { CatalogIcon::ChevronRight } else { CatalogIcon::ChevronDown }).xsmall().text_color(theme.muted_foreground))
                .child(div().font_semibold().child(format!("Not started here ({count})")))
                .into_any_element(),
            Some(Row::Entry(ix)) => {
                let entry = &self.entries[ix];
                match column.as_ref() {
                    "name" => div().min_w_0().truncate().child(entry.name.clone()).into_any_element(),
                    "startup" => self.render_switch(&entry.clone()),
                    "status" => Self::render_status(entry, cx),
                    "source" => entry.source.label().into_any_element(),
                    "command" => div().min_w_0().truncate().child(entry.exec.clone().unwrap_or_default()).into_any_element(),
                    _ => div().into_any_element(),
                }
            }
            _ => div().into_any_element(),
        }
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, _: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(entry) = self.id_at(row_ix).and_then(|id| self.entry(&id).cloned()) else {
            return menu;
        };
        // Right-click selects, so every action works on the selected entry.
        self.selected = Some(entry.id.clone());
        cx.notify();

        let missing = entry.status == Status::FileMissing;
        menu.menu_with_disabled(if entry.enabled { "Don't start at login" } else { "Start at login" }, Box::new(ToggleEnabled), missing)
            .separator()
            .menu_with_disabled("Start now", Box::new(StartNow), !entry.can_start())
            .menu_with_disabled("Stop now", Box::new(StopNow), !entry.can_stop())
            .separator()
            .menu_with_disabled("Open file location", Box::new(OpenFileLocation), missing)
            .menu_with_disabled("Copy command", Box::new(CopyCommand), entry.exec.is_none())
    }
}
