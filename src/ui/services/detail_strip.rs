//! The strip under the table with the selected service's details and the
//! last few lines of its log.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::ServicesPage;
use crate::system::format;

const LABEL_WIDTH: f32 = 88.;
const STRIP_HEIGHT: f32 = 92.;
const COLUMN_WIDTH: f32 = 260.;

impl ServicesPage {
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
        let table = self.table.read(cx).delegate();
        let Some(service) = table.selected.as_deref().and_then(|name| table.service(name)) else {
            return frame
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Select a service to see its details and log")
                .into_any_element();
        };

        let dash = || "—".to_string();
        let details = self.details.as_ref().filter(|(name, _)| *name == service.name).map(|(_, details)| details);
        let since = details.and_then(|details| details.active_since).map(|micros| {
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
            format::ago(now.saturating_sub(Duration::from_micros(micros)))
        });
        let columns: [Vec<(&str, String)>; 2] = [
            vec![
                ("Unit", service.name.clone()),
                ("State", format!("{} ({})", service.state.label(), service.sub)),
                ("Startup", service.startup_label()),
                ("Main PID", service.main_pid.map_or_else(dash, |pid| pid.to_string())),
            ],
            vec![
                ("Memory", service.memory.map_or_else(dash, format::bytes)),
                ("Changed", since.unwrap_or_else(dash)),
                ("File", details.and_then(|details| details.path.clone()).unwrap_or_else(dash)),
                ("Description", if service.description.is_empty() { dash() } else { service.description.clone() }),
            ],
        ];
        let log = match details {
            None => vec![div().text_color(theme.muted_foreground).child("Reading the log…")],
            Some(details) if details.log.is_empty() => vec![div().text_color(theme.muted_foreground).child("Nothing in the journal")],
            Some(details) => details.log.iter().map(|line| div().min_w_0().truncate().child(line.clone())).collect(),
        };

        frame
            .items_center()
            .children(columns.into_iter().map(|pairs| {
                v_flex().w(px(COLUMN_WIDTH)).flex_shrink_0().gap_0p5().children(pairs.into_iter().map(|(label, value)| {
                    h_flex()
                        .gap_2()
                        .text_xs()
                        .child(div().w(px(LABEL_WIDTH)).flex_shrink_0().text_color(theme.muted_foreground).child(label))
                        .child(div().min_w_0().truncate().child(value))
                }))
            }))
            .child(v_flex().flex_1().min_w_0().gap_0p5().text_xs().font_family(theme.mono_font_family.clone()).children(log))
            .into_any_element()
    }
}
