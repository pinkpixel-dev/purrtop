//! The Types tab: every file added up by extension.

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::ShowLargestOfType;
use super::parts::{self, ColumnSpec, Rows};
use crate::system::disk_scan::TypeTotal;
use crate::system::format;

const COLUMNS: [ColumnSpec; 6] = [
    ("extension", "Extension", 160., false),
    ("kind", "Type", 140., false),
    ("size", "Size", 100., true),
    ("share", "Share", 160., false),
    ("files", "Files", 100., true),
    ("average", "Average", 100., true),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortKey {
    Extension,
    Kind,
    Size,
    Files,
    Average,
}

pub struct TypesTable {
    types: Vec<TypeTotal>,
    rows: Vec<usize>,
    columns: Vec<Column>,
    sort: (SortKey, bool),
    pub filter: String,
    pub selected: Option<usize>,
    pub total: u64,
}

/// `.mkv`, or `No extension`.
pub fn extension_label(total: &TypeTotal) -> String {
    if total.extension.is_empty() { "No extension".into() } else { format!(".{}", total.extension) }
}

pub fn average(total: &TypeTotal) -> u64 {
    total.size.checked_div(total.files).unwrap_or(0)
}

impl TypesTable {
    pub fn new() -> Self {
        Self { types: Vec::new(), rows: Vec::new(), columns: parts::columns(&COLUMNS), sort: (SortKey::Size, true), filter: String::new(), selected: None, total: 0 }
    }

    pub fn set_types(&mut self, types: Vec<TypeTotal>, total: u64) {
        self.types = types;
        self.total = total;
        self.selected = None;
        self.rebuild();
    }

    pub fn len(&self) -> usize {
        self.types.len()
    }

    pub fn selected_type(&self) -> Option<&TypeTotal> {
        self.selected.and_then(|ix| self.types.get(ix))
    }

    /// Takes a trashed file off its extension's total. Trashed folders only
    /// come off `total`, since what was inside them by type isn't kept.
    pub fn remove(&mut self, extension: Option<&str>, size: u64, files: u64) {
        if let Some(entry) = extension.and_then(|extension| self.types.iter_mut().find(|total| total.extension == extension)) {
            entry.size = entry.size.saturating_sub(size);
            entry.files = entry.files.saturating_sub(files);
        }
        self.types.retain(|total| total.files > 0);
        self.total = self.total.saturating_sub(size);
        self.selected = None;
        self.rebuild();
    }

    pub fn rebuild(&mut self) {
        let filter = self.filter.trim().trim_start_matches('.').to_lowercase();
        let mut rows: Vec<usize> = (0..self.types.len())
            .filter(|&ix| filter.is_empty() || self.types[ix].extension.contains(&filter) || self.types[ix].kind.label().to_lowercase().contains(&filter))
            .collect();
        let (key, descending) = self.sort;
        let types = &self.types;
        rows.sort_by(|&a, &b| {
            let (a, b) = (&types[a], &types[b]);
            let ordering = match key {
                SortKey::Extension => a.extension.cmp(&b.extension),
                SortKey::Kind => a.kind.cmp(&b.kind).then_with(|| b.size.cmp(&a.size)),
                SortKey::Size => a.size.cmp(&b.size),
                SortKey::Files => a.files.cmp(&b.files),
                SortKey::Average => average(a).cmp(&average(b)),
            }
            .then_with(|| a.extension.cmp(&b.extension));
            if descending { ordering.reverse() } else { ordering }
        });
        self.rows = rows;
    }
}

impl Rows for TypesTable {
    fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn selected_row(&self) -> Option<usize> {
        self.selected.and_then(|ix| self.rows.iter().position(|&row| row == ix))
    }

    fn select_row(&mut self, row: usize) {
        self.selected = self.rows.get(row).copied();
    }
}

impl TableDelegate for TypesTable {
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
        let key = match self.columns[col_ix].key.as_ref() {
            "extension" => SortKey::Extension,
            "kind" => SortKey::Kind,
            "files" => SortKey::Files,
            "average" => SortKey::Average,
            _ => SortKey::Size,
        };
        self.sort = match sort {
            ColumnSort::Ascending => (key, false),
            ColumnSort::Descending => (key, true),
            _ => (SortKey::Size, true),
        };
        self.rebuild();
    }

    fn render_tr(&mut self, row_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> Stateful<Div> {
        let theme = cx.theme();
        let tr = div().id(("disk-types-row", row_ix));
        let Some(&ix) = self.rows.get(row_ix) else {
            return tr;
        };
        tr.when(self.selected == Some(ix), |this| this.bg(theme.list_active)).on_click(cx.listener(move |table, _, _, cx| {
            table.delegate_mut().selected = Some(ix);
            cx.notify();
        }))
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let theme = cx.theme();
        let Some(total) = self.rows.get(row_ix).map(|&ix| &self.types[ix]) else {
            return div().into_any_element();
        };
        let muted = |text: String| div().text_color(theme.muted_foreground).child(text).into_any_element();
        match self.columns[col_ix].key.as_ref() {
            "extension" => div().min_w_0().truncate().child(extension_label(total)).into_any_element(),
            "kind" => parts::with_dot(total.kind, total.kind.label().to_string(), cx),
            "size" => format::bytes(total.size).into_any_element(),
            "share" => parts::share_bar(parts::fraction(total.size, self.total), parts::kind_color(total.kind, cx), cx),
            "files" => muted(total.files.to_string()),
            "average" => muted(format::bytes(average(total))),
            _ => div().into_any_element(),
        }
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, _: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(&ix) = self.rows.get(row_ix) else {
            return menu;
        };
        self.selected = Some(ix);
        cx.notify();
        menu.menu_with_disabled("Show largest files", Box::new(ShowLargestOfType), self.types[ix].extension.is_empty())
    }
}
