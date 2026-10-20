//! The Folders tab: the largest folders, counting everything inside them.

use std::path::Path;

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::parts::{self, ColumnSpec, Rows};
use super::{CopyPath, MoveToTrash, OpenLocation};
use crate::system::disk_scan::FolderEntry;
use crate::system::format;

const COLUMNS: [ColumnSpec; 5] = [
    ("name", "Name", 260., false),
    ("size", "Size", 100., true),
    ("share", "Share", 160., false),
    ("files", "Files", 100., true),
    ("location", "Location", 420., false),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SortKey {
    Name,
    Size,
    Files,
    Location,
}

pub struct FoldersTable {
    folders: Vec<FolderEntry>,
    rows: Vec<usize>,
    columns: Vec<Column>,
    sort: (SortKey, bool),
    pub filter: String,
    pub selected: Option<usize>,
    /// The whole scan's size, for the share column.
    pub total: u64,
}

impl FoldersTable {
    pub fn new() -> Self {
        Self { folders: Vec::new(), rows: Vec::new(), columns: parts::columns(&COLUMNS), sort: (SortKey::Size, true), filter: String::new(), selected: None, total: 0 }
    }

    pub fn set_folders(&mut self, folders: Vec<FolderEntry>, total: u64) {
        self.folders = folders;
        self.total = total;
        self.selected = None;
        self.rebuild();
    }

    pub fn len(&self) -> usize {
        self.folders.len()
    }

    pub fn selected_folder(&self) -> Option<&FolderEntry> {
        self.selected.and_then(|ix| self.folders.get(ix))
    }

    /// After `path` went to the Trash: drops it and what's under it, and
    /// takes its size and files off every folder it was in.
    pub fn remove(&mut self, path: &Path, size: u64, files: u64, is_file: bool) {
        let row = self.selected_row();
        self.folders.retain(|folder| !folder.path.starts_with(path));
        for folder in self.folders.iter_mut().filter(|folder| path.starts_with(&folder.path)) {
            folder.size = folder.size.saturating_sub(size);
            folder.files = folder.files.saturating_sub(files);
            if is_file && path.parent() == Some(folder.path.as_path()) {
                folder.direct = folder.direct.saturating_sub(size);
            }
        }
        self.total = self.total.saturating_sub(size);
        self.folders.sort_by(|a, b| b.size.cmp(&a.size));
        self.rebuild();
        self.selected = row.and_then(|row| self.rows.get(row.min(self.rows.len().saturating_sub(1))).copied());
    }

    pub fn rebuild(&mut self) {
        let filter = self.filter.trim().to_lowercase();
        let mut rows: Vec<usize> =
            (0..self.folders.len()).filter(|&ix| filter.is_empty() || self.folders[ix].path.to_string_lossy().to_lowercase().contains(&filter)).collect();
        let (key, descending) = self.sort;
        let folders = &self.folders;
        rows.sort_by(|&a, &b| {
            let (a, b) = (&folders[a], &folders[b]);
            let ordering = match key {
                SortKey::Name => parts::file_name(&a.path).to_lowercase().cmp(&parts::file_name(&b.path).to_lowercase()),
                SortKey::Size => a.size.cmp(&b.size),
                SortKey::Files => a.files.cmp(&b.files),
                SortKey::Location => a.path.parent().cmp(&b.path.parent()),
            }
            .then_with(|| a.path.cmp(&b.path));
            if descending { ordering.reverse() } else { ordering }
        });
        self.rows = rows;
    }
}

impl Rows for FoldersTable {
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

impl TableDelegate for FoldersTable {
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
            "files" => SortKey::Files,
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
        let tr = div().id(("disk-folders-row", row_ix));
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
        let Some(folder) = self.rows.get(row_ix).map(|&ix| &self.folders[ix]) else {
            return div().into_any_element();
        };
        match self.columns[col_ix].key.as_ref() {
            "name" => div().min_w_0().truncate().child(parts::file_name(&folder.path)).into_any_element(),
            "size" => format::bytes(folder.size).into_any_element(),
            "share" => parts::share_bar(parts::fraction(folder.size, self.total), theme.primary, cx),
            "files" => div().text_color(theme.muted_foreground).child(folder.files.to_string()).into_any_element(),
            "location" => div().min_w_0().truncate().text_color(theme.muted_foreground).child(parts::location(&folder.path)).into_any_element(),
            _ => div().into_any_element(),
        }
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, _: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(&ix) = self.rows.get(row_ix) else {
            return menu;
        };
        self.selected = Some(ix);
        cx.notify();
        menu.menu("Open location", Box::new(OpenLocation)).menu("Copy path", Box::new(CopyPath)).separator().menu("Move to Trash", Box::new(MoveToTrash))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::FoldersTable;
    use crate::system::disk_scan::FolderEntry;

    fn folder(path: &str, size: u64, direct: u64, files: u64) -> FolderEntry {
        FolderEntry { path: PathBuf::from(path), size, direct, files }
    }

    #[test]
    fn trashing_takes_size_off_the_parents() {
        let mut table = FoldersTable::new();
        table.set_folders(vec![folder("/r/a", 100, 10, 9), folder("/r/a/b", 60, 0, 5), folder("/r/a/b/c", 40, 40, 4), folder("/r/z", 30, 30, 1)], 140);

        table.remove(&PathBuf::from("/r/a/b"), 60, 5, false);
        let left: Vec<_> = table.folders.iter().map(|folder| (folder.path.to_string_lossy().into_owned(), folder.size, folder.files)).collect();
        assert_eq!(left, [("/r/a".into(), 40, 4), ("/r/z".into(), 30, 1)]);
        assert_eq!(table.total, 80);

        // A file sitting directly in a folder also comes off its direct size.
        table.remove(&PathBuf::from("/r/a/big.iso"), 10, 1, true);
        assert_eq!(table.folders[0].direct, 0);
        assert_eq!(table.folders[0].size, 30);
    }
}
