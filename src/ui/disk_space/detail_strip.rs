//! The strip under the table with the selected file, folder or type's details.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::files::modified;
use super::parts::{self, display_path};
use super::toolbar::thousands;
use super::types::{average, extension_label};
use super::{DiskSpacePage, SpaceTab};
use crate::system::format;

const LABEL_WIDTH: f32 = 88.;
const STRIP_HEIGHT: f32 = 92.;
const COLUMN_WIDTH: f32 = 260.;

type Pairs = Vec<(&'static str, String)>;

impl DiskSpacePage {
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
        let Some((columns, path)) = self.details(cx) else {
            return frame.justify_center().text_sm().text_color(theme.muted_foreground).child("Select a row to see its details").into_any_element();
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
            .when_some(path, |this, path| {
                this.child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_0p5()
                        .text_xs()
                        .child(div().text_color(theme.muted_foreground).child("Path"))
                        .child(div().min_w_0().line_clamp(3).font_family(theme.mono_font_family.clone()).child(path)),
                )
            })
            .into_any_element()
    }

    /// Two columns of facts and the full path, for whatever's selected.
    fn details(&self, cx: &App) -> Option<([Pairs; 2], Option<String>)> {
        let total = self.summary.as_ref().map_or(0, |summary| summary.counts.bytes);
        let share = |size: u64| parts::percent(parts::fraction(size, total));
        let dash = || "—".to_string();
        match self.tab {
            SpaceTab::Bubbles | SpaceTab::Files => {
                let file = self.files.read(cx).delegate().selected_file()?;
                Some((
                    [
                        vec![
                            ("Name", parts::file_name(&file.path)),
                            ("Size on disk", format::bytes(file.size)),
                            ("Length", format::bytes(file.apparent)),
                            ("Share", share(file.size)),
                        ],
                        vec![
                            ("Type", file.kind.label().to_string()),
                            ("Extension", if file.extension.is_empty() { dash() } else { format!(".{}", file.extension) }),
                            ("Modified", modified(file).unwrap_or_else(dash)),
                        ],
                    ],
                    Some(display_path(&file.path)),
                ))
            }
            SpaceTab::Folders => {
                let folder = self.folders.read(cx).delegate().selected_folder()?;
                Some((
                    [
                        vec![("Name", parts::file_name(&folder.path)), ("Size", format::bytes(folder.size)), ("Share", share(folder.size))],
                        vec![("Files inside", thousands(folder.files)), ("Directly in it", format::bytes(folder.direct))],
                    ],
                    Some(display_path(&folder.path)),
                ))
            }
            SpaceTab::Types => {
                let kind = self.types.read(cx).delegate().selected_type()?;
                Some((
                    [
                        vec![("Extension", extension_label(kind)), ("Type", kind.kind.label().to_string()), ("Size", format::bytes(kind.size))],
                        vec![("Share", share(kind.size)), ("Files", thousands(kind.files)), ("Average", format::bytes(average(kind)))],
                    ],
                    None,
                ))
            }
        }
    }
}
