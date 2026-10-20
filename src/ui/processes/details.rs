//! The "Linux process details" dialog: everything `/proc` shows about one process.

use gpui_kit::component::button::Button;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tab::TabBar;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::*;

use crate::system::details::ProcessDetails;
use crate::system::format;
use crate::system::processes::ProcessInfo;

const DIALOG_WIDTH: f32 = 760.;
const BODY_HEIGHT: f32 = 460.;
const KEY_WIDTH: f32 = 180.;
const TABS: [&str; 5] = ["Overview", "Status", "Limits", "Environment", "Open files"];

pub fn open(process: ProcessInfo, window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }
    let details = ProcessDetails::read(process.pid);
    let view = cx.new(|_| DetailsView { process, details, tab: 0 });
    window.open_dialog(cx, move |dialog, _, cx| {
        let title = {
            let process = &view.read(cx).process;
            format!("{} (PID {})", process.name, process.pid)
        };
        dialog.w(px(DIALOG_WIDTH)).title(title).child(view.clone()).footer(
            h_flex()
                .w_full()
                .justify_end()
                .child(Button::new("details-close").outline().label("Close").on_click(|_, window, cx| window.close_dialog(cx))),
        )
    });
}

struct DetailsView {
    process: ProcessInfo,
    details: ProcessDetails,
    tab: usize,
}

impl DetailsView {
    fn overview(&self) -> Vec<(String, String)> {
        let p = &self.process;
        let unknown = || "Access denied".to_string();
        vec![
            ("Name".into(), p.name.clone()),
            ("PID".into(), p.pid.to_string()),
            ("Parent PID".into(), p.ppid.to_string()),
            ("User".into(), format!("{} ({})", p.user, p.uid)),
            ("Status".into(), p.state_label().into()),
            ("Type".into(), format!("{:?}", p.kind)),
            ("Threads".into(), p.threads.to_string()),
            ("Nice".into(), p.nice.to_string()),
            ("CPU time".into(), format!("{:.2} s user · {:.2} s kernel", p.user_time, p.system_time)),
            ("Memory".into(), format!("{} resident · {} virtual", format::bytes(p.memory), format::bytes(p.virtual_memory))),
            ("Executable".into(), p.exe.clone().unwrap_or_else(unknown)),
            ("Working folder".into(), self.details.cwd.clone().unwrap_or_else(|err| err)),
            ("Cgroup".into(), p.cgroup.clone()),
            ("Command line".into(), p.command.clone()),
        ]
    }

    fn body(&self, cx: &App) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let pairs = |pairs: Vec<(String, String)>| {
            v_flex()
                .gap_1()
                .children(pairs.into_iter().map(|(key, value)| {
                    h_flex()
                        .gap_3()
                        .items_start()
                        .text_sm()
                        .child(div().w(px(KEY_WIDTH)).flex_shrink_0().text_color(muted).child(key))
                        .child(div().flex_1().min_w_0().child(value))
                }))
                .into_any_element()
        };
        let lines = |lines: Vec<String>| v_flex().gap_0p5().text_sm().children(lines).into_any_element();
        let denied = |err: &String| div().text_sm().text_color(muted).child(err.clone()).into_any_element();

        match self.tab {
            0 => pairs(self.overview()),
            1 => pairs(self.details.status.clone()),
            2 => match &self.details.limits {
                Ok(limits) => pairs(
                    limits.iter().map(|[name, soft, hard, units]| (name.clone(), format!("soft {soft} · hard {hard} {units}"))).collect(),
                ),
                Err(err) => denied(err),
            },
            3 => match &self.details.environment {
                Ok(vars) if vars.is_empty() => lines(vec!["No environment variables".into()]),
                Ok(vars) => lines(vars.clone()),
                Err(err) => denied(err),
            },
            _ => match &self.details.open_files {
                Ok(files) => pairs(files.clone()),
                Err(err) => denied(err),
            },
        }
    }
}

impl Render for DetailsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        v_flex()
            .gap_3()
            .child(
                TabBar::new("details-tabs").underline().selected_index(self.tab).children(TABS).on_click(move |&ix, _, cx| {
                    _ = view.update(cx, |view, cx| {
                        view.tab = ix;
                        cx.notify();
                    });
                }),
            )
            .child(div().id("details-body").h(px(BODY_HEIGHT)).overflow_y_scrollbar().child(self.body(cx)))
    }
}
