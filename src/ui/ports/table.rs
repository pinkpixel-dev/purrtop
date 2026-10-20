//! The ports table: one row per listening socket, lowest port first.

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::*;
use crate::system::ports::Socket;

/// `(key, title, width, right aligned)` per column, left to right.
const COLUMNS: [(&str, &str, f32, bool); 6] = [
    ("port", "Port", 90., true),
    ("protocol", "Protocol", 90., false),
    ("address", "Address", 240., false),
    ("process", "Process", 260., false),
    ("pid", "PID", 90., true),
    ("user", "User", 140., false),
];

/// A socket and what's known about whoever holds it.
#[derive(Clone, Debug)]
pub struct Port {
    pub socket: Socket,
    /// Every process holding the socket, empty when they need admin rights to see.
    pub pids: Vec<u32>,
    /// The one to show and act on, from [`ports::main_pid`].
    pub pid: Option<u32>,
    pub name: Option<String>,
    pub command: Option<String>,
    pub user: String,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    #[default]
    Port,
    Protocol,
    Address,
    Process,
    Pid,
    User,
}

pub struct PortsTable {
    ports: Vec<Port>,
    /// Indices into `ports`, filtered and sorted.
    rows: Vec<usize>,
    columns: Vec<Column>,
    sort: (SortKey, bool),
    pub filter: String,
    /// Tracked by socket inode, since rows move as ports come and go.
    pub selected: Option<u64>,
    pub loaded: bool,
}

impl PortsTable {
    pub fn new() -> Self {
        let columns = COLUMNS
            .iter()
            .map(|&(key, title, width, right)| {
                let column = Column::new(key, title).width(width).sortable();
                if right { column.text_right() } else { column }
            })
            .collect();
        Self { ports: Vec::new(), rows: Vec::new(), columns, sort: Default::default(), filter: String::new(), selected: None, loaded: false }
    }

    pub fn set_ports(&mut self, ports: Vec<Port>) {
        self.ports = ports;
        self.loaded = true;
        self.rebuild();
    }

    pub fn ports(&self) -> &[Port] {
        &self.ports
    }

    pub fn port(&self, inode: u64) -> Option<&Port> {
        self.ports.iter().find(|port| port.socket.inode == inode)
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn inode_at(&self, row: usize) -> Option<u64> {
        self.rows.get(row).map(|&ix| self.ports[ix].socket.inode)
    }

    pub fn row_of(&self, inode: u64) -> Option<usize> {
        self.rows.iter().position(|&ix| self.ports[ix].socket.inode == inode)
    }

    pub fn rebuild(&mut self) {
        let filter = self.filter.trim().to_lowercase();
        let mut rows: Vec<usize> = (0..self.ports.len()).filter(|&ix| filter.is_empty() || matches(&self.ports[ix], &filter)).collect();
        let (key, descending) = self.sort;
        rows.sort_by(|&a, &b| {
            let (a, b) = (&self.ports[a], &self.ports[b]);
            let ordering = match key {
                SortKey::Port => std::cmp::Ordering::Equal,
                SortKey::Protocol => a.socket.protocol_label().cmp(b.socket.protocol_label()),
                SortKey::Address => a.socket.address.ip().cmp(&b.socket.address.ip()),
                SortKey::Process => lowercase(&a.name).cmp(&lowercase(&b.name)),
                SortKey::Pid => a.pid.cmp(&b.pid),
                SortKey::User => a.user.cmp(&b.user),
            }
            .then_with(|| a.socket.address.port().cmp(&b.socket.address.port()))
            .then_with(|| a.socket.protocol_label().cmp(b.socket.protocol_label()));
            if descending { ordering.reverse() } else { ordering }
        });
        self.rows = rows;
    }
}

fn lowercase(name: &Option<String>) -> Option<String> {
    name.as_ref().map(|name| name.to_lowercase())
}

/// Matches the port, address, process name or user.
fn matches(port: &Port, filter: &str) -> bool {
    port.socket.address.port().to_string().starts_with(filter)
        || port.socket.address.ip().to_string().contains(filter)
        || port.name.as_ref().is_some_and(|name| name.to_lowercase().contains(filter))
        || port.user.to_lowercase().contains(filter)
}

impl TableDelegate for PortsTable {
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
            "protocol" => SortKey::Protocol,
            "address" => SortKey::Address,
            "process" => SortKey::Process,
            "pid" => SortKey::Pid,
            "user" => SortKey::User,
            _ => SortKey::Port,
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
        let tr = div().id(("ports-row", row_ix));
        let Some(inode) = self.inode_at(row_ix) else {
            return tr;
        };
        tr.when(self.selected == Some(inode), |this| this.bg(theme.list_active)).on_click(cx.listener(move |table, _, _, cx| {
            table.delegate_mut().selected = Some(inode);
            cx.notify();
        }))
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let theme = cx.theme();
        let Some(&ix) = self.rows.get(row_ix) else {
            return div().into_any_element();
        };
        let port = &self.ports[ix];
        let muted = |text: &str| div().text_color(theme.muted_foreground).child(text.to_string()).into_any_element();
        match self.columns[col_ix].key.as_ref() {
            "port" => port.socket.address.port().to_string().into_any_element(),
            "protocol" => muted(port.socket.protocol_label()),
            "address" => div().min_w_0().truncate().child(port.socket.address.ip().to_string()).into_any_element(),
            "process" => match &port.name {
                Some(name) => div().min_w_0().truncate().child(name.clone()).into_any_element(),
                None if port.pid.is_some() => muted("—"),
                None => muted("Needs admin"),
            },
            "pid" => port.pid.map_or_else(|| muted("—"), |pid| pid.to_string().into_any_element()),
            "user" => div().min_w_0().truncate().child(port.user.clone()).into_any_element(),
            _ => div().into_any_element(),
        }
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, _: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(port) = self.inode_at(row_ix).and_then(|inode| self.port(inode).cloned()) else {
            return menu;
        };
        // Right-click selects, so every action works on the selected port.
        self.selected = Some(port.socket.inode);
        cx.notify();

        // Signalling PID 1 does nothing useful and could do something bad.
        let can_end = port.pid.is_some_and(|pid| pid != 1);
        menu.menu_with_disabled("End process", Box::new(EndProcess), !can_end)
            .menu_with_disabled("Kill process", Box::new(KillProcess), !can_end)
            .separator()
            .menu_with_disabled("Find process as admin", Box::new(FindOwners), port.pid.is_some())
            .menu_with_disabled("Open in Processes", Box::new(OpenInProcesses), port.pid.is_none())
            .menu("Copy address", Box::new(CopyAddress))
    }
}
