//! The services table: failed ones first, then everything running or set to
//! start, then a folded group of stopped services that won't start on their own.

use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::*;
use crate::system::format;
use crate::system::services::{Service, State};

/// `(key, title, width, right aligned)` per column, left to right.
const COLUMNS: [(&str, &str, f32, bool); 6] = [
    ("name", "Name", 260., false),
    ("description", "Description", 320., false),
    ("state", "State", 96., false),
    ("startup", "Startup", 110., false),
    ("pid", "PID", 80., true),
    ("memory", "Memory", 100., true),
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortKey {
    /// Failed first, then by name.
    #[default]
    State,
    Name,
    Description,
    Startup,
    Pid,
    Memory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Service(usize),
    Group { count: usize, collapsed: bool },
}

pub struct ServicesTable {
    services: Vec<Service>,
    rows: Vec<Row>,
    columns: Vec<Column>,
    sort: (SortKey, bool),
    pub filter: String,
    pub idle_collapsed: bool,
    /// Tracked by unit name, since rows move as states change.
    pub selected: Option<String>,
    pub loaded: bool,
}

impl ServicesTable {
    pub fn new() -> Self {
        let columns = COLUMNS
            .iter()
            .map(|&(key, title, width, right)| {
                let column = Column::new(key, title).width(width).sortable();
                if right { column.text_right() } else { column }
            })
            .collect();
        Self { services: Vec::new(), rows: Vec::new(), columns, sort: Default::default(), filter: String::new(), idle_collapsed: true, selected: None, loaded: false }
    }

    pub fn set_services(&mut self, services: Vec<Service>) {
        self.services = services;
        self.loaded = true;
        self.rebuild();
    }

    /// Empties the table while another scope loads.
    pub fn clear(&mut self) {
        self.services.clear();
        self.selected = None;
        self.loaded = false;
        self.rebuild();
    }

    pub fn services(&self) -> &[Service] {
        &self.services
    }

    pub fn service(&self, name: &str) -> Option<&Service> {
        self.services.iter().find(|service| service.name == name)
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn name_at(&self, row: usize) -> Option<String> {
        match self.rows.get(row)? {
            Row::Service(ix) => Some(self.services[*ix].name.clone()),
            Row::Group { .. } => None,
        }
    }

    pub fn row_of(&self, name: &str) -> Option<usize> {
        self.rows.iter().position(|row| matches!(row, Row::Service(ix) if self.services[*ix].name == name))
    }

    pub fn toggle_idle(&mut self) {
        self.idle_collapsed = !self.idle_collapsed;
        self.rebuild();
    }

    pub fn rebuild(&mut self) {
        let filter = self.filter.trim().to_lowercase();
        let mut order: Vec<usize> = (0..self.services.len())
            .filter(|&ix| {
                let service = &self.services[ix];
                filter.is_empty() || service.name.to_lowercase().contains(&filter) || service.description.to_lowercase().contains(&filter)
            })
            .collect();
        let (key, descending) = self.sort;
        order.sort_by(|&a, &b| {
            let (a, b) = (&self.services[a], &self.services[b]);
            let ordering = match key {
                SortKey::State => a.state.cmp(&b.state),
                SortKey::Name => std::cmp::Ordering::Equal,
                SortKey::Description => a.description.to_lowercase().cmp(&b.description.to_lowercase()),
                SortKey::Startup => a.startup.cmp(&b.startup),
                SortKey::Pid => a.main_pid.cmp(&b.main_pid),
                SortKey::Memory => a.memory.cmp(&b.memory),
            }
            .then_with(|| a.name.cmp(&b.name));
            if descending { ordering.reverse() } else { ordering }
        });
        let (idle, main): (Vec<usize>, Vec<usize>) = order.into_iter().partition(|&ix| self.services[ix].is_idle());
        let mut rows: Vec<Row> = main.into_iter().map(Row::Service).collect();
        if !idle.is_empty() {
            // A filter opens the group, so matches are never hidden.
            let collapsed = self.idle_collapsed && filter.is_empty();
            rows.push(Row::Group { count: idle.len(), collapsed });
            if !collapsed {
                rows.extend(idle.into_iter().map(Row::Service));
            }
        }
        self.rows = rows;
    }

    fn render_state(service: &Service, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let color = match service.state {
            State::Failed => theme.danger,
            State::Running => theme.success,
            State::Starting | State::Stopping => theme.warning,
            State::Exited | State::Inactive => theme.muted_foreground,
        };
        div().text_color(color).child(service.state.label()).into_any_element()
    }
}

impl TableDelegate for ServicesTable {
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
            "description" => SortKey::Description,
            "startup" => SortKey::Startup,
            "pid" => SortKey::Pid,
            "memory" => SortKey::Memory,
            _ => SortKey::State,
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
        let tr = div().id(("services-row", row_ix));
        match self.rows.get(row_ix).copied() {
            Some(Row::Group { .. }) => tr.cursor_pointer().on_click(cx.listener(|table, _, _, cx| {
                table.delegate_mut().toggle_idle();
                cx.notify();
            })),
            Some(Row::Service(ix)) => {
                let name = self.services[ix].name.clone();
                let selected = self.selected.as_deref() == Some(name.as_str());
                tr.when(selected, |this| this.bg(theme.list_active)).on_click(cx.listener(move |table, _, _, cx| {
                    table.delegate_mut().selected = Some(name.clone());
                    cx.notify();
                }))
            }
            None => tr,
        }
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let theme = cx.theme();
        let column = self.columns[col_ix].key.clone();
        let muted = |text: &str| div().text_color(theme.muted_foreground).child(text.to_string()).into_any_element();
        match self.rows.get(row_ix).copied() {
            Some(Row::Group { count, collapsed }) if column.as_ref() == "name" => h_flex()
                .gap_1()
                .child(Icon::new(if collapsed { CatalogIcon::ChevronRight } else { CatalogIcon::ChevronDown }).xsmall().text_color(theme.muted_foreground))
                .child(div().font_semibold().child(format!("Stopped ({count})")))
                .into_any_element(),
            Some(Row::Service(ix)) => {
                let service = &self.services[ix];
                match column.as_ref() {
                    "name" => div().min_w_0().truncate().child(service.name.trim_end_matches(".service").to_string()).into_any_element(),
                    "description" => div().min_w_0().truncate().text_color(theme.muted_foreground).child(service.description.clone()).into_any_element(),
                    "state" => Self::render_state(service, cx),
                    "startup" if service.startup.is_none() => muted("—"),
                    "startup" => service.startup_label().into_any_element(),
                    "pid" => service.main_pid.map_or_else(|| muted("—"), |pid| pid.to_string().into_any_element()),
                    "memory" => service.memory.map_or_else(|| muted("—"), |bytes| format::bytes(bytes).into_any_element()),
                    _ => div().into_any_element(),
                }
            }
            _ => div().into_any_element(),
        }
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, _: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(service) = self.name_at(row_ix).and_then(|name| self.service(&name).cloned()) else {
            return menu;
        };
        // Right-click selects, so every action works on the selected service.
        self.selected = Some(service.name.clone());
        cx.notify();

        let active = service.state.is_active();
        menu.menu_with_disabled("Start", Box::new(Start), active || service.is_masked())
            .menu_with_disabled("Stop", Box::new(Stop), !active)
            .menu_with_disabled("Restart", Box::new(Restart), service.is_masked())
            .separator()
            .menu_with_disabled("Enable", Box::new(Enable), !service.can_enable())
            .menu_with_disabled("Disable", Box::new(Disable), !service.can_disable())
            .separator()
            .menu_with_disabled("Open in Processes", Box::new(OpenInProcesses), service.main_pid.is_none())
            .menu("Copy name", Box::new(CopyName))
    }
}
