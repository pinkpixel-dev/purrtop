//! The strip under the table with the selected process's numbers.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::ProcessesPage;
use crate::system::format;

const LABEL_WIDTH: f32 = 96.;
const STRIP_HEIGHT: f32 = 92.;

impl ProcessesPage {
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
        let Some(process) = table.selected.and_then(|pid| table.process(pid)) else {
            return frame
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Select a process to see its details")
                .into_any_element();
        };

        let opt = |value: Option<u64>| value.map_or("—".to_string(), format::bytes);
        let columns: [Vec<(&str, String)>; 4] = [
            vec![
                ("PID", process.pid.to_string()),
                ("Parent", process.ppid.to_string()),
                ("User", process.user.clone()),
                ("Threads", process.threads.to_string()),
            ],
            vec![
                ("CPU time", format!("{:.2} s", process.user_time + process.system_time)),
                ("Kernel time", format!("{:.2} s", process.system_time)),
                ("User time", format!("{:.2} s", process.user_time)),
                ("Memory", format::bytes(process.memory)),
            ],
            vec![
                ("Peak memory", opt(process.peak_memory)),
                ("Virtual", format::bytes(process.virtual_memory)),
                ("Shared", opt(process.shared_memory)),
                ("Swap", opt(process.swap)),
            ],
            vec![
                ("Open files", process.open_files.map_or("—".to_string(), |count| count.to_string())),
                ("Nice", process.nice.to_string()),
                ("Cgroup", process.cgroup.clone()),
                ("Image path", process.exe.clone().unwrap_or_else(|| "Access denied".into())),
            ],
        ];

        frame
            .items_center()
            .children(columns.into_iter().enumerate().map(|(ix, pairs)| {
                // The last column holds long paths, so it gets the spare room.
                let column = if ix == 3 { v_flex().flex_1().min_w_0() } else { v_flex().flex_shrink_0() };
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

