//! The Add to startup dialog: search your installed apps and pick one to
//! start at login. Clicking an app or pressing Enter adds it.

use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::button::Button;
use gpui_kit::component::list::{List, ListDelegate, ListItem, ListState};
use gpui_kit::component::{ActiveTheme as _, IndexPath, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;

use crate::system::launchers::{self, Launcher};

const DIALOG_WIDTH: f32 = 520.;
const LIST_HEIGHT: f32 = 380.;

type OnAdd = Rc<dyn Fn(Launcher, &mut Window, &mut App)>;

/// Opens the dialog. Apps whose file name is in `taken` are already in the
/// list, so they're left out.
pub fn open(taken: HashSet<String>, on_add: impl Fn(Launcher, &mut Window, &mut App) + 'static, window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    let delegate = AppList { all: Vec::new(), matches: Vec::new(), query: String::new(), selected: None, loaded: false, on_add: Rc::new(on_add) };
    let list = cx.new(|cx| ListState::new(delegate, window, cx).searchable(true));
    list.update(cx, |_, cx| {
        cx.spawn(async move |this, cx| {
            let launchers = cx.background_spawn(async move { launchers::read().into_iter().filter(|launcher| !taken.contains(&launcher.id)).collect() }).await;
            _ = this.update(cx, |state, cx| {
                state.delegate_mut().set_launchers(launchers);
                cx.notify();
            });
        })
        .detach();
    });
    let dialog_list = list.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .w(px(DIALOG_WIDTH))
            .title("Add to startup")
            .child(div().h(px(LIST_HEIGHT)).child(List::new(&dialog_list).search_placeholder("Search installed apps")))
            .footer(h_flex().w_full().justify_end().child(Button::new("add-startup-cancel").outline().label("Cancel").on_click(|_, window, cx| window.close_dialog(cx))))
    });
    list.update(cx, |list, cx| list.focus(window, cx));
}

struct AppList {
    all: Vec<Launcher>,
    /// Indices into `all` that match the search.
    matches: Vec<usize>,
    query: String,
    /// A row in `matches`.
    selected: Option<usize>,
    loaded: bool,
    on_add: OnAdd,
}

impl AppList {
    fn set_launchers(&mut self, launchers: Vec<Launcher>) {
        self.all = launchers;
        self.loaded = true;
        self.filter();
    }

    /// Matches the name, description or command, ignoring case.
    fn filter(&mut self) {
        let query = self.query.trim().to_lowercase();
        self.matches = (0..self.all.len())
            .filter(|&ix| {
                let launcher = &self.all[ix];
                query.is_empty()
                    || launcher.name.to_lowercase().contains(&query)
                    || launcher.comment.as_ref().is_some_and(|comment| comment.to_lowercase().contains(&query))
                    || launcher.exec.to_lowercase().contains(&query)
            })
            .collect();
        self.selected = self.selected.filter(|&row| row < self.matches.len());
    }
}

impl ListDelegate for AppList {
    type Item = ListItem;

    fn perform_search(&mut self, query: &str, _: &mut Window, cx: &mut Context<ListState<Self>>) -> Task<()> {
        self.query = query.to_string();
        self.filter();
        cx.notify();
        Task::ready(())
    }

    fn items_count(&self, _: usize, _: &App) -> usize {
        self.matches.len()
    }

    fn render_item(&mut self, ix: IndexPath, _: &mut Window, cx: &mut Context<ListState<Self>>) -> Option<Self::Item> {
        let launcher = &self.all[*self.matches.get(ix.row)?];
        // Every row has two lines, since the list needs rows of one height.
        let detail = launcher.comment.clone().unwrap_or_else(|| launcher.exec.clone());
        Some(
            ListItem::new(("startup-app", ix.row)).selected(self.selected == Some(ix.row)).accessibility_label(launcher.name.clone()).child(
                v_flex()
                    .min_w_0()
                    .child(div().text_sm().truncate().child(launcher.name.clone()))
                    .child(div().text_xs().truncate().text_color(cx.theme().muted_foreground).child(detail)),
            ),
        )
    }

    fn render_empty(&mut self, _: &mut Window, cx: &mut Context<ListState<Self>>) -> impl IntoElement {
        let text = if self.all.is_empty() { "Every installed app is already in the list" } else { "No apps match" };
        h_flex().size_full().justify_center().text_sm().text_color(cx.theme().muted_foreground).child(text)
    }

    fn loading(&self, _: &App) -> bool {
        !self.loaded
    }

    fn set_selected_index(&mut self, ix: Option<IndexPath>, _: &mut Window, cx: &mut Context<ListState<Self>>) {
        self.selected = ix.map(|ix| ix.row);
        cx.notify();
    }

    fn confirm(&mut self, _: bool, window: &mut Window, cx: &mut Context<ListState<Self>>) {
        let Some(launcher) = self.selected.and_then(|row| self.matches.get(row)).map(|&ix| self.all[ix].clone()) else {
            return;
        };
        window.close_dialog(cx);
        (self.on_add)(launcher, window, cx);
    }
}
