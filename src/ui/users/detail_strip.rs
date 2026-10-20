//! The strip under the table with the selected user's or process's details.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::UsersPage;
use super::table::Selection;
use crate::system::format;

const LABEL_WIDTH: f32 = 88.;
const STRIP_HEIGHT: f32 = 92.;
const COLUMN_WIDTH: f32 = 280.;

impl UsersPage {
    pub(super) fn render_detail_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let frame = h_flex()
            .h(px(STRIP_HEIGHT))
            .flex_shrink_0()
            .px_3()
            .gap_6()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.group_box);
        let Some(columns) = self.details(cx) else {
            return frame
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Select a user to see its details")
                .into_any_element();
        };

        let last = columns.len() - 1;
        frame
            .items_center()
            .children(columns.into_iter().enumerate().map(|(ix, pairs)| {
                // The last column holds the long session list or command line.
                let column = if ix == last { v_flex().flex_1().min_w_0() } else { v_flex().w(px(COLUMN_WIDTH)).flex_shrink_0() };
                column.gap_0p5().children(pairs.into_iter().map(|(label, value)| {
                    h_flex()
                        .gap_2()
                        .text_xs()
                        .child(div().w(px(LABEL_WIDTH)).flex_shrink_0().text_color(theme.muted_foreground).child(label))
                        .child(div().min_w_0().truncate().child(value))
                }))
            }))
            .into_any_element()
    }

    fn details(&self, cx: &App) -> Option<Vec<Vec<(&'static str, String)>>> {
        let table = self.table.read(cx).delegate();
        let dash = || "—".to_string();
        Some(match table.selected? {
            Selection::User(uid) => {
                let user = table.user(uid)?;
                let first = user.sessions.first();
                let signed_in = first.and_then(|session| session.started).map(|started| format::duration(table.uptime.saturating_sub(started)));
                let busiest = user.processes.first().and_then(|&pid| table.process(pid)).map(|process| process.name.clone());
                let mut sessions: Vec<(&str, String)> =
                    user.sessions.iter().take(4).map(|session| ("Session", format!("{} ({})", session.label(), session.status()))).collect();
                if sessions.is_empty() {
                    sessions.push(("Session", "Not signed in".into()));
                }
                vec![
                    vec![
                        ("User", user.name.clone()),
                        ("UID", user.uid.to_string()),
                        ("Account", if user.is_person() { "Person" } else { "System" }.to_string()),
                        ("Signed in for", signed_in.unwrap_or_else(dash)),
                    ],
                    vec![
                        ("CPU", format::percent(user.cpu)),
                        ("Memory", format::bytes(user.memory)),
                        ("Processes", user.processes.len().to_string()),
                        ("Busiest", busiest.unwrap_or_else(dash)),
                    ],
                    sessions,
                ]
            }
            Selection::Process(pid) => {
                let process = table.process(pid)?;
                vec![
                    vec![
                        ("Name", process.name.clone()),
                        ("PID", process.pid.to_string()),
                        ("User", process.user.clone()),
                        ("Status", process.state_label().to_string()),
                    ],
                    vec![
                        ("CPU", format::percent(process.cpu)),
                        ("Memory", format::bytes(process.memory)),
                        ("Threads", process.threads.to_string()),
                        ("Nice", process.nice.to_string()),
                    ],
                    vec![("Command", if process.command.is_empty() { dash() } else { process.command.clone() })],
                ]
            }
        })
    }
}
