//! The Disk Space page: scan a folder and see what's taking up the space,
//! as the largest files, the largest folders, and totals by type.

mod actions;
mod detail_strip;
mod files;
mod folders;
mod parts;
mod toolbar;
mod types;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::table::{DataTable, TableDelegate, TableState};
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, v_flex};
use gpui_kit::component::notification::Notification;
use gpui_kit::*;
use serde::Deserialize;

use self::files::FilesTable;
use self::folders::FoldersTable;
use self::parts::Rows;
use self::types::TypesTable;
use crate::system::disk_scan::{self, Counts, Progress, Scan};

const CONTEXT: &str = "DiskSpace";
/// Only while a table has focus, so typing in the filter is untouched.
const TABLE_CONTEXT: &str = "DiskSpace > DataTable";
const PAGE_ROWS: usize = 20;
/// How often the running counts redraw during a scan.
const TICK: Duration = Duration::from_millis(250);

actions!(
    disk_space,
    [
        Rescan,
        StopScan,
        ChooseFolder,
        OpenLocation,
        CopyPath,
        MoveToTrash,
        ShowLargestOfType,
        NextTab,
        PreviousTab,
        SelectNext,
        SelectPrevious,
        SelectFirst,
        SelectLast,
        SelectPageDown,
        SelectPageUp,
        FocusFilter
    ]
);

/// Scans this folder.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = disk_space, no_json)]
pub struct ScanFolder(pub String);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some(TABLE_CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(TABLE_CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(TABLE_CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(TABLE_CONTEXT)),
        KeyBinding::new("pagedown", SelectPageDown, Some(TABLE_CONTEXT)),
        KeyBinding::new("pageup", SelectPageUp, Some(TABLE_CONTEXT)),
        KeyBinding::new("delete", MoveToTrash, Some(TABLE_CONTEXT)),
        KeyBinding::new("enter", OpenLocation, Some(TABLE_CONTEXT)),
        KeyBinding::new("ctrl-f", FocusFilter, Some(CONTEXT)),
        KeyBinding::new("ctrl-tab", NextTab, Some(CONTEXT)),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, Some(CONTEXT)),
        KeyBinding::new("f5", Rescan, Some(CONTEXT)),
        KeyBinding::new("escape", StopScan, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SpaceTab {
    #[default]
    Files,
    Folders,
    Types,
}

impl SpaceTab {
    const ALL: [SpaceTab; 3] = [SpaceTab::Files, SpaceTab::Folders, SpaceTab::Types];

    fn label(self) -> &'static str {
        match self {
            SpaceTab::Files => "Largest files",
            SpaceTab::Folders => "Largest folders",
            SpaceTab::Types => "Types",
        }
    }
}

/// What the last finished scan covered.
struct Summary {
    counts: Counts,
    elapsed: Duration,
    stopped: bool,
}

/// A scan that's still running.
struct Scanning {
    root: PathBuf,
    progress: Arc<Progress>,
    started: Instant,
    /// Redraws the counts; dropped when the scan ends.
    _ticker: Task<()>,
}

/// Moves the selection in whichever table is showing.
#[derive(Clone, Copy)]
enum Move {
    By(isize),
    First,
    Last,
}

pub struct DiskSpacePage {
    files: Entity<TableState<FilesTable>>,
    folders: Entity<TableState<FoldersTable>>,
    types: Entity<TableState<TypesTable>>,
    filter: Entity<InputState>,
    tab: SpaceTab,
    /// The folder the next scan reads.
    root: PathBuf,
    summary: Option<Summary>,
    scanning: Option<Scanning>,
    error: Option<String>,
    focus_handle: FocusHandle,
    _subscription: Subscription,
}

impl DiskSpacePage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let files = cx.new(|cx| TableState::new(FilesTable::new(), window, cx).row_selectable(false).col_movable(false));
        let folders = cx.new(|cx| TableState::new(FoldersTable::new(), window, cx).row_selectable(false).col_movable(false));
        let types = cx.new(|cx| TableState::new(TypesTable::new(), window, cx).row_selectable(false).col_movable(false));
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by name or path"));
        let subscription = cx.subscribe_in(&filter, window, |this, filter, event: &InputEvent, window, cx| match event {
            InputEvent::Change => {
                let text = filter.read(cx).value().to_string();
                this.set_filter(text, cx);
            }
            // Enter hands the keyboard to the table, ready for arrow keys.
            InputEvent::PressEnter { .. } => this.focus_table(window, cx),
            _ => {}
        });
        let root = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
        Self { files, folders, types, filter, tab: SpaceTab::default(), root, summary: None, scanning: None, error: None, focus_handle: cx.focus_handle(), _subscription: subscription }
    }

    /// Showing the page hands it the keyboard. Scans keep running while it's hidden.
    pub fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if active {
            self.focus_table(window, cx);
        }
    }

    fn focus_table(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.summary.is_none() || self.scanning.is_some() {
            self.focus_handle.focus(window, cx);
            return;
        }
        match self.tab {
            SpaceTab::Files => self.files.read(cx).focus_handle(cx).focus(window, cx),
            SpaceTab::Folders => self.folders.read(cx).focus_handle(cx).focus(window, cx),
            SpaceTab::Types => self.types.read(cx).focus_handle(cx).focus(window, cx),
        }
    }

    /// Starts scanning `root`, stopping any scan that's already running.
    fn start_scan(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(scanning) = self.scanning.take() {
            scanning.progress.cancel();
        }
        self.root = root.clone();
        self.error = None;
        let progress = Arc::new(Progress::default());
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        self.scanning = Some(Scanning { root: root.clone(), progress: progress.clone(), started: Instant::now(), _ticker: ticker });
        self.focus_handle.focus(window, cx);
        cx.spawn_in(window, async move |this, cx| {
            let job = progress.clone();
            let result = cx.background_spawn(async move { disk_scan::scan(&root, &job) }).await;
            _ = this.update_in(cx, |this, window, cx| this.finish(&progress, result, window, cx));
        })
        .detach();
        cx.notify();
    }

    fn finish(&mut self, progress: &Arc<Progress>, result: Result<Scan, String>, window: &mut Window, cx: &mut Context<Self>) {
        // A newer scan took over; this one's numbers don't matter anymore.
        if !self.scanning.as_ref().is_some_and(|scanning| Arc::ptr_eq(&scanning.progress, progress)) {
            return;
        }
        self.scanning = None;
        match result {
            Ok(scan) => {
                let total = scan.counts.bytes;
                self.files.update(cx, |state, cx| {
                    state.delegate_mut().set_files(scan.files);
                    cx.notify();
                });
                self.folders.update(cx, |state, cx| {
                    state.delegate_mut().set_folders(scan.folders, total);
                    cx.notify();
                });
                self.types.update(cx, |state, cx| {
                    state.delegate_mut().set_types(scan.types, total);
                    cx.notify();
                });
                self.summary = Some(Summary { counts: scan.counts, elapsed: scan.elapsed, stopped: scan.stopped });
            }
            Err(err) => self.error = Some(err),
        }
        self.focus_table(window, cx);
        cx.notify();
    }

    fn set_filter(&mut self, text: String, cx: &mut Context<Self>) {
        let apply = |filter: &mut String| *filter = text.clone();
        self.files.update(cx, |state, cx| {
            apply(&mut state.delegate_mut().filter);
            state.delegate_mut().rebuild();
            cx.notify();
        });
        self.folders.update(cx, |state, cx| {
            apply(&mut state.delegate_mut().filter);
            state.delegate_mut().rebuild();
            cx.notify();
        });
        self.types.update(cx, |state, cx| {
            apply(&mut state.delegate_mut().filter);
            state.delegate_mut().rebuild();
            cx.notify();
        });
        cx.notify();
    }

    fn rescan(&mut self, _: &Rescan, window: &mut Window, cx: &mut Context<Self>) {
        if self.scanning.is_none() {
            self.start_scan(self.root.clone(), window, cx);
        }
    }

    fn stop_scan(&mut self, _: &StopScan, _: &mut Window, _: &mut Context<Self>) {
        if let Some(scanning) = &self.scanning {
            scanning.progress.cancel();
        }
    }

    fn scan_folder(&mut self, action: &ScanFolder, window: &mut Window, cx: &mut Context<Self>) {
        self.start_scan(PathBuf::from(&action.0), window, cx);
    }

    /// Opens the desktop's folder picker and scans what you pick.
    fn choose_folder(&mut self, _: &ChooseFolder, window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: Some("Scan".into()) });
        cx.spawn_in(window, async move |this, cx| {
            let result = picked.await;
            _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.start_scan(path, window, cx);
                    }
                }
                Ok(Err(err)) => window.push_notification(Notification::error(format!("Couldn't open the folder picker: {err}")), cx),
                _ => {}
            });
        })
        .detach();
    }

    fn select_tab(&mut self, tab: SpaceTab, window: &mut Window, cx: &mut Context<Self>) {
        self.tab = tab;
        self.focus_table(window, cx);
        cx.notify();
    }

    fn step_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let tabs = SpaceTab::ALL;
        let current = tabs.iter().position(|&tab| tab == self.tab).unwrap_or(0);
        let next = if forward { (current + 1) % tabs.len() } else { (current + tabs.len() - 1) % tabs.len() };
        self.select_tab(tabs[next], window, cx);
    }

    fn next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        self.step_tab(true, window, cx);
    }

    fn previous_tab(&mut self, _: &PreviousTab, window: &mut Window, cx: &mut Context<Self>) {
        self.step_tab(false, window, cx);
    }

    fn move_selection(&mut self, to: Move, cx: &mut Context<Self>) {
        match self.tab {
            SpaceTab::Files => move_in(&self.files, to, cx),
            SpaceTab::Folders => move_in(&self.folders, to, cx),
            SpaceTab::Types => move_in(&self.types, to, cx),
        }
        cx.notify();
    }

    fn select_next(&mut self, _: &SelectNext, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(Move::By(1), cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(Move::By(-1), cx);
    }

    fn select_page_down(&mut self, _: &SelectPageDown, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(Move::By(PAGE_ROWS as isize), cx);
    }

    fn select_page_up(&mut self, _: &SelectPageUp, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(Move::By(-(PAGE_ROWS as isize)), cx);
    }

    fn select_first(&mut self, _: &SelectFirst, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(Move::First, cx);
    }

    fn select_last(&mut self, _: &SelectLast, _: &mut Window, cx: &mut Context<Self>) {
        self.move_selection(Move::Last, cx);
    }

    fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.update(cx, |filter, cx| filter.focus(window, cx));
    }

    fn render_results(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let selected = SpaceTab::ALL.iter().position(|&tab| tab == self.tab).unwrap_or(0);
        let counts = [self.files.read(cx).delegate().len(), self.folders.read(cx).delegate().len(), self.types.read(cx).delegate().len()];
        let tabs = TabBar::new("disk-space-tabs")
            .underline()
            .selected_index(selected)
            .children(SpaceTab::ALL.iter().zip(counts).map(|(tab, count)| Tab::new().label(format!("{} ({count})", tab.label()))))
            .on_click(cx.listener(|this, ix: &usize, window, cx| this.select_tab(SpaceTab::ALL[(*ix).min(SpaceTab::ALL.len() - 1)], window, cx)));
        let table = match self.tab {
            SpaceTab::Files => DataTable::new(&self.files).bordered(false).into_any_element(),
            SpaceTab::Folders => DataTable::new(&self.folders).bordered(false).into_any_element(),
            SpaceTab::Types => DataTable::new(&self.types).bordered(false).into_any_element(),
        };
        v_flex()
            .flex_1()
            .min_h_0()
            .gap_3()
            .child(div().flex_shrink_0().child(tabs))
            .child(div().flex_1().min_h_0().rounded(theme.radius_lg).border_1().border_color(theme.border).overflow_hidden().child(table))
            .child(self.render_detail_strip(cx))
    }
}

/// Moves the selection in one table and keeps it in view.
fn move_in<D: TableDelegate + Rows>(table: &Entity<TableState<D>>, to: Move, cx: &mut App) {
    table.update(cx, |state, cx| {
        let rows = state.delegate().row_count();
        if rows == 0 {
            return;
        }
        let row = match to {
            Move::By(delta) => state.delegate().selected_row().map_or(0, |row| row.saturating_add_signed(delta)),
            Move::First => 0,
            Move::Last => rows - 1,
        }
        .min(rows - 1);
        state.delegate_mut().select_row(row);
        state.vertical_scroll_handle.scroll_to_item(row, ScrollStrategy::Nearest);
        cx.notify();
    });
}

impl Focusable for DiskSpacePage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for DiskSpacePage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.scanning.is_some() {
            self.render_progress(cx).into_any_element()
        } else if let Some(error) = &self.error {
            let error = error.clone();
            self.render_message(error, true, cx).into_any_element()
        } else if self.summary.is_some() {
            self.render_results(cx).into_any_element()
        } else {
            self.render_empty(cx).into_any_element()
        };
        v_flex()
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
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
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::previous_tab))
            .on_action(cx.listener(Self::rescan))
            .on_action(cx.listener(Self::stop_scan))
            .on_action(cx.listener(Self::scan_folder))
            .on_action(cx.listener(Self::choose_folder))
            .on_action(cx.listener(Self::open_location))
            .on_action(cx.listener(Self::copy_path))
            .on_action(cx.listener(Self::move_to_trash))
            .on_action(cx.listener(Self::show_largest_of_type))
            .child(self.render_toolbar(cx))
            .child(v_flex().flex_1().min_h_0().child(body))
    }
}
