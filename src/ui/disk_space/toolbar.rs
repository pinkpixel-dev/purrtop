//! The bar across the top (what to scan, scan or stop, the filter) and the
//! bodies shown before and during a scan.

use std::path::PathBuf;

use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::parts::display_path;
use super::{ChooseFolder, DiskSpacePage, Rescan, ScanFolder, StopScan};
use crate::system::disk_info;
use crate::system::format;

const FILTER_WIDTH: f32 = 260.;
const ROOT_WIDTH: f32 = 280.;

/// `1,234,567`.
pub fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (ix, digit) in digits.chars().enumerate() {
        if ix > 0 && (digits.len() - ix) % 3 == 0 {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// `0.8 s`, `38 s`, or `2 min 5 s`.
pub fn seconds(duration: std::time::Duration) -> String {
    let secs = duration.as_secs_f32();
    match secs {
        ..10. => format!("{secs:.1} s"),
        ..60. => format!("{secs:.0} s"),
        _ => format!("{} min {} s", duration.as_secs() / 60, duration.as_secs() % 60),
    }
}

impl DiskSpacePage {
    pub(super) fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let scanning = self.scanning.is_some();
        let summary = self.summary.as_ref().filter(|_| !scanning).map(|summary| {
            let mut text = format!("{} · {} files · {}", format::bytes(summary.counts.bytes), thousands(summary.counts.files), seconds(summary.elapsed));
            if summary.counts.skipped > 0 {
                text.push_str(&format!(" · {} folders skipped", thousands(summary.counts.skipped)));
            }
            (text, summary.stopped)
        });
        let home = std::env::var("HOME").unwrap_or_default();

        h_flex()
            .flex_shrink_0()
            .gap_3()
            .items_center()
            .child(div().text_lg().font_semibold().child("Disk Space"))
            .when_some(summary, |this, (text, stopped)| {
                this.child(div().text_xs().text_color(theme.muted_foreground).child(text))
                    .when(stopped, |this| this.child(div().text_xs().text_color(theme.warning).child("Stopped early")))
            })
            .child(div().flex_1())
            .child(
                Button::new("disk-space-root")
                    .ghost()
                    .small()
                    .icon(Icon::new(CatalogIcon::Folder))
                    .child(div().max_w(px(ROOT_WIDTH)).truncate().child(display_path(&self.root)))
                    .tooltip("Pick what to scan")
                    .dropdown_menu(move |mut menu, _, _| {
                        if !home.is_empty() {
                            menu = menu.menu("Home folder", Box::new(ScanFolder(home.clone()))).separator();
                        }
                        for mount in disk_info::mount_points() {
                            menu = menu.menu(mount.clone(), Box::new(ScanFolder(mount)));
                        }
                        menu.separator().menu("Choose folder…", Box::new(ChooseFolder))
                    }),
            )
            .map(|this| {
                if scanning {
                    this.child(
                        Button::new("disk-space-stop")
                            .small()
                            .danger()
                            .icon(Icon::new(CatalogIcon::Square))
                            .label("Stop")
                            .tooltip("Stop and show what's been found (Esc)")
                            .on_click(cx.listener(|this, _, window, cx| this.stop_scan(&StopScan, window, cx))),
                    )
                } else {
                    this.child(
                        Button::new("disk-space-scan")
                            .small()
                            .primary()
                            .icon(Icon::new(CatalogIcon::RefreshCw))
                            .label(if self.summary.is_some() { "Rescan" } else { "Scan" })
                            .tooltip("Scan the folder (F5)")
                            .on_click(cx.listener(|this, _, window, cx| this.rescan(&Rescan, window, cx))),
                    )
                }
            })
            .when(self.summary.is_some() && !scanning, |this| {
                this.child(
                    div().w(px(FILTER_WIDTH)).child(
                        Input::new(&self.filter)
                            .small()
                            .cleanable(true)
                            .prefix(Icon::new(IconName::Search).small().text_color(theme.muted_foreground)),
                    ),
                )
            })
    }

    /// Live counts while a scan runs.
    pub(super) fn render_progress(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let Some(scanning) = &self.scanning else {
            return div();
        };
        let counts = scanning.progress.counts();
        let stats = [
            ("Found", format::bytes(counts.bytes)),
            ("Files", thousands(counts.files)),
            ("Folders", thousands(counts.folders)),
            ("Time", seconds(scanning.started.elapsed())),
        ];
        self.panel(cx).child(
            v_flex()
                .gap_5()
                .items_center()
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .text_sm()
                        .child(Icon::new(IconName::Loader).small().text_color(theme.primary))
                        .child(format!("Scanning {}", display_path(&scanning.root))),
                )
                .child(h_flex().gap_10().children(stats.into_iter().map(|(label, value)| {
                    v_flex()
                        .items_center()
                        .gap_0p5()
                        .child(div().text_2xl().font_semibold().child(value))
                        .child(div().text_xs().text_color(theme.muted_foreground).child(label))
                })))
                .when(counts.skipped > 0, |this| {
                    this.child(div().text_xs().text_color(theme.muted_foreground).child(format!("{} folders couldn't be read", thousands(counts.skipped))))
                }),
        )
    }

    /// Before the first scan.
    pub(super) fn render_empty(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let root: PathBuf = self.root.clone();
        self.panel(cx).child(
            v_flex()
                .gap_3()
                .items_center()
                .child(Icon::new(CatalogIcon::ChartPie).large().text_color(theme.muted_foreground))
                .child(div().text_sm().text_color(theme.muted_foreground).child(format!("Scan {} to see what's taking up space", display_path(&root))))
                .child(
                    Button::new("disk-space-start")
                        .primary()
                        .label("Scan")
                        .on_click(cx.listener(|this, _, window, cx| this.rescan(&Rescan, window, cx))),
                ),
        )
    }

    pub(super) fn render_message(&self, message: String, error: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let color = if error { cx.theme().danger } else { cx.theme().muted_foreground };
        self.panel(cx).child(div().text_sm().text_color(color).child(message))
    }

    /// The bordered box the empty, progress and error bodies sit in.
    fn panel(&self, cx: &App) -> Div {
        let theme = cx.theme();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.group_box)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{seconds, thousands};

    #[test]
    fn formats_counts_and_times() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(seconds(Duration::from_millis(800)), "0.8 s");
        assert_eq!(seconds(Duration::from_secs(38)), "38 s");
        assert_eq!(seconds(Duration::from_secs(125)), "2 min 5 s");
    }
}
