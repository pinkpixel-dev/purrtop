//! The strip under the table with the selected port's details.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::PortsPage;

const LABEL_WIDTH: f32 = 88.;
const STRIP_HEIGHT: f32 = 92.;
const COLUMN_WIDTH: f32 = 300.;

impl PortsPage {
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
        let Some(port) = table.selected.and_then(|inode| table.port(inode)) else {
            return frame.justify_center().text_sm().text_color(theme.muted_foreground).child("Select a port to see its details").into_any_element();
        };

        let dash = || "—".to_string();
        let others: Vec<String> = port.pids.iter().filter(|&&pid| Some(pid) != port.pid).map(u32::to_string).collect();
        let columns: [Vec<(&str, String)>; 2] = [
            vec![
                ("Address", port.socket.address.to_string()),
                ("Protocol", port.socket.protocol_label().to_string()),
                ("Reachable", port.socket.reach().to_string()),
                ("User", port.user.clone()),
            ],
            vec![
                ("Process", port.name.clone().unwrap_or_else(|| if port.pid.is_some() { dash() } else { "Needs admin to see".into() })),
                ("PID", port.pid.map_or_else(dash, |pid| pid.to_string())),
                ("Also held by", if others.is_empty() { dash() } else { others.join(", ") }),
                ("Socket", port.socket.inode.to_string()),
            ],
        ];
        let command = port.command.clone().filter(|command| !command.is_empty()).unwrap_or_else(dash);

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
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .text_xs()
                    .child(div().text_color(theme.muted_foreground).child("Command"))
                    .child(div().min_w_0().line_clamp(3).font_family(theme.mono_font_family.clone()).child(command)),
            )
            .into_any_element()
    }
}
