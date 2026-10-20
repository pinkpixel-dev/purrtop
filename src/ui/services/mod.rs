//! The Services page: systemd services from the system or your user
//! instance, kept live over D-Bus, with start, stop, restart, enable and disable.

mod actions;
mod detail_strip;
mod table;

use std::time::Duration;

use futures_lite::StreamExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::table::{DataTable, TableState};
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde::Deserialize;

use self::table::ServicesTable;
use crate::system::services::{self, Details, Scope, State, Systemd};

const CONTEXT: &str = "Services";
/// Only while the table itself has focus, so typing in the filter is untouched.
const TABLE_CONTEXT: &str = "Services > DataTable";
const FILTER_WIDTH: f32 = 280.;
const PAGE_ROWS: usize = 20;
/// Re-reads the list this often even without signals, for memory use.
const REFRESH: Duration = Duration::from_secs(5);
/// Signals come in bursts (one per property), so wait for them to settle.
const SETTLE: Duration = Duration::from_millis(750);
const LOG_LINES: usize = 4;

actions!(
    services,
    [Start, Stop, Restart, Enable, Disable, OpenInProcesses, CopyName, SelectNext, SelectPrevious, SelectFirst, SelectLast, SelectPageDown, SelectPageUp, FocusFilter]
);

/// Shows the system or the user instance.
#[derive(Clone, PartialEq, Deserialize, Action)]
#[action(namespace = services, no_json)]
pub struct ShowUserServices(pub bool);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNext, Some(TABLE_CONTEXT)),
        KeyBinding::new("up", SelectPrevious, Some(TABLE_CONTEXT)),
        KeyBinding::new("home", SelectFirst, Some(TABLE_CONTEXT)),
        KeyBinding::new("end", SelectLast, Some(TABLE_CONTEXT)),
        KeyBinding::new("pagedown", SelectPageDown, Some(TABLE_CONTEXT)),
        KeyBinding::new("pageup", SelectPageUp, Some(TABLE_CONTEXT)),
        KeyBinding::new("ctrl-f", FocusFilter, Some(CONTEXT)),
    ]);
}

pub struct ServicesPage {
    table: Entity<TableState<ServicesTable>>,
    filter: Entity<InputState>,
    scope: Scope,
    systemd: Option<Systemd>,
    /// Set when systemd can't be reached or listed.
    error: Option<SharedString>,
    /// The selected service's details, by name.
    details: Option<(String, Details)>,
    /// Lists services and waits for changes while the page is showing.
    watch_task: Option<Task<()>>,
    details_task: Option<Task<()>>,
    active: bool,
    _subscriptions: Vec<Subscription>,
}

impl ServicesPage {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table = cx.new(|cx| TableState::new(ServicesTable::new(), window, cx).row_selectable(false).col_movable(false));
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by name or description"));
        let subscriptions = vec![
            cx.observe(&table, |this, _, cx| this.selection_changed(cx)),
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
        Self {
            table,
            filter,
            scope: Scope::default(),
            systemd: None,
            error: None,
            details: None,
            watch_task: None,
            details_task: None,
            active: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.active == active {
            return;
        }
        self.active = active;
        if active {
            self.table.read(cx).focus_handle(cx).focus(window, cx);
        }
        self.watch(cx);
    }

    /// Connects to systemd and keeps the list fresh: again after each burst
    /// of change signals, and every few seconds for memory. Stops when the
    /// page is hidden, since dropping the task drops the connection.
    fn watch(&mut self, cx: &mut Context<Self>) {
        if !self.active {
            self.watch_task = None;
            return;
        }
        let scope = self.scope;
        self.watch_task = Some(cx.spawn(async move |this, cx| {
            let systemd = match Systemd::connect(scope).await {
                Ok(systemd) => systemd,
                Err(err) => {
                    _ = this.update(cx, |this, cx| this.set_error(format!("Couldn't reach systemd: {err}"), cx));
                    return;
                }
            };
            let mut changes = systemd.changes().await.ok();
            _ = this.update(cx, |this, _| this.systemd = Some(systemd.clone()));
            loop {
                let listed = systemd.list().await;
                let updated = this.update(cx, |this, cx| match listed {
                    Ok(services) => {
                        this.error = None;
                        this.update_table(cx, |table| table.set_services(services));
                    }
                    Err(err) => this.set_error(format!("Couldn't list services: {err}"), cx),
                });
                if updated.is_err() {
                    break;
                }
                let timer = cx.background_executor().timer(REFRESH);
                let Some(stream) = changes.as_mut() else {
                    timer.await;
                    continue;
                };
                let signal = async {
                    stream.next().await;
                };
                futures_lite::future::or(signal, timer).await;
                cx.background_executor().timer(SETTLE).await;
                // Drop whatever arrived meanwhile; one re-read covers it all.
                while futures_lite::future::poll_once(stream.next()).await.flatten().is_some() {}
            }
        }));
    }

    fn set_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.error = Some(error.into());
        self.update_table(cx, |table| table.loaded = true);
    }

    fn set_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        if self.scope == scope {
            return;
        }
        self.scope = scope;
        self.systemd = None;
        self.error = None;
        self.details = None;
        self.update_table(cx, ServicesTable::clear);
        self.watch(cx);
    }

    fn show_user_services(&mut self, action: &ShowUserServices, _: &mut Window, cx: &mut Context<Self>) {
        self.set_scope(if action.0 { Scope::User } else { Scope::System }, cx);
    }

    /// Reads the newly selected service's details and log off the UI thread.
    fn selection_changed(&mut self, cx: &mut Context<Self>) {
        let selected = self.table.read(cx).delegate().selected.clone();
        let current = self.details.as_ref().map(|(name, _)| name);
        if selected.as_ref() == current {
            return;
        }
        self.details = None;
        let (Some(name), Some(systemd)) = (selected, self.systemd.clone()) else {
            self.details_task = None;
            return;
        };
        self.details_task = Some(cx.spawn(async move |this, cx| {
            let mut details = systemd.details(&name).await;
            let (scope, unit) = (systemd.scope(), name.clone());
            details.log = cx.background_spawn(async move { services::journal(scope, &unit, LOG_LINES) }).await;
            _ = this.update(cx, |this, cx| {
                this.details = Some((name, details));
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn update_table(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut ServicesTable)) {
        self.table.update(cx, |state, cx| {
            f(state.delegate_mut());
            cx.notify();
        });
        cx.notify();
    }

    /// Moves the selection to a row, skipping the group row in the direction of travel.
    fn select_row(&mut self, row: usize, forward: bool, cx: &mut Context<Self>) {
        self.table.update(cx, |state, cx| {
            let table = state.delegate_mut();
            let rows = table.rows().len();
            if rows == 0 {
                return;
            }
            let mut row = row.min(rows - 1);
            while table.name_at(row).is_none() {
                match (forward, row) {
                    (true, r) if r + 1 < rows => row += 1,
                    (false, r) if r > 0 => row -= 1,
                    _ => return,
                }
            }
            table.selected = table.name_at(row);
            state.vertical_scroll_handle.scroll_to_item(row, ScrollStrategy::Nearest);
            cx.notify();
        });
        cx.notify();
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let table = self.table.read(cx).delegate();
        let row = match table.selected.as_deref().and_then(|name| table.row_of(name)) {
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

    fn focus_filter(&mut self, _: &FocusFilter, window: &mut Window, cx: &mut Context<Self>) {
        self.filter.update(cx, |filter, cx| filter.focus(window, cx));
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let services = self.table.read(cx).delegate().services();
        let running = services.iter().filter(|service| service.state == State::Running).count();
        let failed = services.iter().filter(|service| service.state == State::Failed).count();
        let theme = cx.theme();
        let user = self.scope == Scope::User;
        h_flex()
            .gap_3()
            .items_center()
            .child(div().text_lg().font_semibold().child("Services"))
            .child(div().text_xs().text_color(theme.muted_foreground).child(format!("{running} running")))
            .when(failed > 0, |this| this.child(div().text_xs().text_color(theme.danger).child(format!("{failed} failed"))))
            .child(div().flex_1())
            .child(
                Button::new("services-scope")
                    .ghost()
                    .small()
                    .label(self.scope.label())
                    .icon(Icon::new(IconName::ChevronDown))
                    .tooltip("System services or your own user services")
                    .dropdown_menu(move |menu, _, _| {
                        menu.menu_with_check("System", !user, Box::new(ShowUserServices(false))).menu_with_check("User", user, Box::new(ShowUserServices(true)))
                    }),
            )
            .child(
                div().w(px(FILTER_WIDTH)).child(
                    Input::new(&self.filter)
                        .small()
                        .cleanable(true)
                        .prefix(Icon::new(IconName::Search).small().text_color(theme.muted_foreground)),
                ),
            )
    }
}

impl Render for ServicesPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (radius, border) = (theme.radius_lg, theme.border);
        let body = match &self.error {
            Some(error) => div().size_full().flex().items_center().justify_center().text_sm().text_color(theme.danger).child(error.clone()).into_any_element(),
            None => DataTable::new(&self.table).bordered(false).into_any_element(),
        };
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
            .on_action(cx.listener(Self::focus_filter))
            .on_action(cx.listener(Self::show_user_services))
            .on_action(cx.listener(Self::start))
            .on_action(cx.listener(Self::stop))
            .on_action(cx.listener(Self::restart))
            .on_action(cx.listener(Self::enable))
            .on_action(cx.listener(Self::disable))
            .on_action(cx.listener(Self::open_in_processes))
            .on_action(cx.listener(Self::copy_name))
            .child(self.render_toolbar(cx))
            .child(div().flex_1().min_h_0().rounded(radius).border_1().border_color(border).overflow_hidden().child(body))
            .child(self.render_detail_strip(cx))
    }
}
