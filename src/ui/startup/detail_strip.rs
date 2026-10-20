//! The strip under the table with the selected entry's details.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::StartupPage;

const LABEL_WIDTH: f32 = 88.;
const STRIP_HEIGHT: f32 = 92.;
/// Wide enough for most unit names, which are long and escaped.
const MIDDLE_WIDTH: f32 = 360.;

impl StartupPage {
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
        let Some(entry) = table.selected.as_deref().and_then(|id| table.entry(id)) else {
            return frame
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Select an entry to see its details")
                .into_any_element();
        };

        let dash = || "—".to_string();
        let columns: [Vec<(&str, String)>; 3] = [
            vec![
                ("Source", entry.source.label().to_string()),
                ("At login", if entry.enabled { "On" } else { "Off" }.to_string()),
                ("Desktop", entry.elsewhere.clone().unwrap_or_else(|| "This desktop".into())),
                ("Delay", entry.delay.map_or_else(dash, |seconds| format!("{seconds} s"))),
            ],
            vec![
                ("Status", entry.status.label().to_string()),
                ("Unit", entry.unit.clone()),
                ("Terminal", if entry.terminal { "Yes" } else { "No" }.to_string()),
                ("Description", entry.comment.clone().unwrap_or_else(dash)),
            ],
            vec![
                ("Name", entry.name.clone()),
                ("Command", entry.exec.clone().unwrap_or_else(dash)),
                ("File", entry.path.display().to_string()),
                ("Links to", entry.link.as_ref().map_or_else(dash, |link| link.display().to_string())),
            ],
        ];

        frame
            .items_center()
            .children(columns.into_iter().enumerate().map(|(ix, pairs)| {
                // The middle column holds the long unit name and the last long paths.
                let column = match ix {
                    0 => v_flex().flex_shrink_0(),
                    1 => v_flex().w(px(MIDDLE_WIDTH)).flex_shrink_0(),
                    _ => v_flex().flex_1().min_w_0(),
                };
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
}
