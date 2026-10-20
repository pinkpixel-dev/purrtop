//! The Files tab: the largest files the scan found.

use std::path::Path;
use std::time::SystemTime;

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::parts::{self, ColumnSpec, Rows};
use super::{CopyPath, MoveToTrash, OpenLocation};
use crate::system::disk_scan::FileEntry;
use crate::system::format;

const COLUMNS: [ColumnSpec; 5] = [
    ("name", "Name", 300., false),
    ("size", "Size", 100., true),
    ("kind", "Type", 120., false),
    ("modified", "Modified", 120., false),
    ("location", "Location", 420., false),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortKey {
    Name,
    Size,
    Kind,
    Modified,
    Location,
}

pub struct FilesTable {
    files: Vec<FileEntry>,
    /// Indices into `files`, filtered and sorted.
    rows: Vec<usize>,
    columns: Vec<Column>,
    /// Key and descending. Largest first until a header is clicked.
    sort: (SortKey, bool),
    pub filter: String,
    /// An index into `files`.
    pub selected: Option<usize>,
}

impl FilesTable {
    pub fn new() -> Self {
        Self { files: Vec::new(), rows: Vec::new(), columns: parts::columns(&COLUMNS), sort: (SortKey::Size, true), filter: String::new(), selected: None }
    }

    pub fn set_files(&mut self, files: Vec<FileEntry>) {
        self.files = files;
        self.selected = None;
        self.rebuild();
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn selected_file(&self) -> Option<&FileEntry> {
        self.selected.and_then(|ix| self.files.get(ix))
    }

    /// Drops `path` and anything under it after it went to the Trash.
    pub fn remove(&mut self, path: &Path) {
        let row = self.selected_row();
        self.files.retain(|file| !file.path.starts_with(path));
        self.rebuild();
        self.selected = row.and_then(|row| self.rows.get(row.min(self.rows.len().saturating_sub(1))).copied());
    }

    pub fn rebuild(&mut self) {
        let filter = self.filter.trim().to_lowercase();
        let mut rows: Vec<usize> =
            (0..self.files.len()).filter(|&ix| filter.is_empty() || self.files[ix].path.to_string_lossy().to_lowercase().contains(&filter)).collect();
        let (key, descending) = self.sort;
        let files = &self.files;
        rows.sort_by(|&a, &b| {
            let (a, b) = (&files[a], &files[b]);
            let ordering = match key {
                SortKey::Name => parts::file_name(&a.path).to_lowercase().cmp(&parts::file_name(&b.path).to_lowercase()),
                SortKey::Size => a.size.cmp(&b.size),
                SortKey::Kind => a.kind.cmp(&b.kind).then_with(|| a.extension.cmp(&b.extension)),
                SortKey::Modified => a.modified.cmp(&b.modified),
                SortKey::Location => a.path.parent().cmp(&b.path.parent()),
            }
            .then_with(|| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
            if descending { ordering.reverse() } else { ordering }
        });
        self.rows = rows;
    }
}

impl Rows for FilesTable {
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

/// `3 days ago`, from the file's modified time.
pub fn modified(file: &FileEntry) -> Option<String> {
    let elapsed = SystemTime::now().duration_since(file.modified?).unwrap_or_default();
    Some(format::ago(elapsed))
}

impl TableDelegate for FilesTable {
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
            "name" => SortKey::Name,
            "kind" => SortKey::Kind,
            "modified" => SortKey::Modified,
            "location" => SortKey::Location,
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
        let tr = div().id(("disk-files-row", row_ix));
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
        let Some(file) = self.rows.get(row_ix).map(|&ix| &self.files[ix]) else {
            return div().into_any_element();
        };
        let muted = |text: String| div().min_w_0().truncate().text_color(theme.muted_foreground).child(text).into_any_element();
        match self.columns[col_ix].key.as_ref() {
            "name" => parts::with_dot(file.kind, parts::file_name(&file.path), cx),
            "size" => format::bytes(file.size).into_any_element(),
            "kind" => muted(file.kind.label().to_string()),
            "modified" => muted(modified(file).unwrap_or_else(|| "—".into())),
            "location" => muted(parts::location(&file.path)),
            _ => div().into_any_element(),
        }
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, _: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(&ix) = self.rows.get(row_ix) else {
            return menu;
        };
        // Right-click selects, so every action works on the selected file.
        self.selected = Some(ix);
        cx.notify();
        menu.menu("Open location", Box::new(OpenLocation)).menu("Copy path", Box::new(CopyPath)).separator().menu("Move to Trash", Box::new(MoveToTrash))
    }
}
