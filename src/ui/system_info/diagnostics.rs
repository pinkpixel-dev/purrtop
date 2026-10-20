//! The Diagnostics tab: failed units, errors this boot, recent crashes and OOM
//! kills, each as a short list with an all-clear or error line when empty.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gpui_kit::assets::IconName as CatalogIcon;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::*;

use crate::system::diagnostics::{Diagnostics, signal_name};
use crate::system::format;

/// Error groups shown before the list gets cut off.
const ERROR_ROWS: usize = 100;
const COUNT_WIDTH: f32 = 52.;
const SOURCE_WIDTH: f32 = 180.;
const TIME_WIDTH: f32 = 96.;
const TAG_WIDTH: f32 = 96.;

pub fn render(diagnostics: Option<&Diagnostics>, cx: &App) -> Vec<AnyElement> {
    let Some(diagnostics) = diagnostics else {
        let note = div().text_sm().text_color(cx.theme().muted_foreground).child("Reading the journal…");
        return vec![note.into_any_element()];
    };
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let ago = |time_us: u64| format::ago(now.saturating_sub(Duration::from_micros(time_us)));

    let failed = match &diagnostics.failed_units {
        Ok(units) if units.is_empty() => all_clear("No failed units", cx),
        Ok(units) => rows(units.iter().map(|unit| {
            row(cx)
                .child(cell_flex(unit.name.clone(), true, cx))
                .child(cell_flex(unit.description.clone(), false, cx))
                .child(cell(TAG_WIDTH, if unit.user { "User" } else { "System" }, true, cx))
        })),
        Err(err) => problem(err, cx),
    };
    let failed_count = diagnostics.failed_units.as_ref().ok().map(|units| units.len().to_string());

    let errors = match &diagnostics.errors {
        Ok(summary) if summary.groups.is_empty() => all_clear("No errors since boot", cx),
        Ok(summary) => {
            let mut list: Vec<AnyElement> = summary
                .groups
                .iter()
                .take(ERROR_ROWS)
                .enumerate()
                .map(|(ix, group)| {
                    row(cx)
                        .child(div().w(px(COUNT_WIDTH)).flex_shrink_0().text_right().text_color(cx.theme().muted_foreground).child(format!("{}×", group.count)))
                        .child(cell(SOURCE_WIDTH, group.source.clone(), false, cx).font_semibold())
                        .child(
                            div()
                                .id(("error-message", ix))
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .child(group.message.clone())
                                .tooltip({
                                    let message = group.message.clone();
                                    move |window, cx| Tooltip::new(message.clone()).build(window, cx)
                                }),
                        )
                        .child(time(ago(group.last_us), cx))
                        .into_any_element()
                })
                .collect();
            if summary.groups.len() > ERROR_ROWS {
                let more = summary.groups.len() - ERROR_ROWS;
                list.push(div().text_xs().text_color(cx.theme().muted_foreground).child(format!("and {more} older messages")).into_any_element());
            }
            v_flex().gap_1().children(list).into_any_element()
        }
        Err(err) => problem(err, cx),
    };
    let error_count = diagnostics.errors.as_ref().ok().map(|summary| match summary.groups.len() {
        0 => "0".to_string(),
        kinds => format!("{} messages, {kinds} kinds", summary.total),
    });

    let crashes = match &diagnostics.crashes {
        Ok(crashes) if crashes.is_empty() => all_clear("No crashes in the last 7 days", cx),
        Ok(crashes) => rows(crashes.iter().enumerate().map(|(ix, crash)| {
            let name = crash.exe.rsplit('/').next().unwrap_or(&crash.exe).to_string();
            row(cx)
                .child(
                    div()
                        .id(("crash-exe", ix))
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_semibold()
                        .child(name)
                        .tooltip({
                            let exe = crash.exe.clone();
                            move |window, cx| Tooltip::new(exe.clone()).build(window, cx)
                        }),
                )
                .child(cell(TAG_WIDTH, crash.signal.map_or_else(|| "Unknown".into(), signal_name), true, cx))
                .child(cell(TAG_WIDTH, format!("PID {}", crash.pid), true, cx))
                .child(time(ago(crash.time_us), cx))
        })),
        Err(err) => problem(err, cx),
    };
    let crash_count = diagnostics.crashes.as_ref().ok().map(|crashes| crashes.len().to_string());

    let oom = match &diagnostics.oom_kills {
        Ok(kills) if kills.is_empty() => all_clear("Nothing killed for memory since boot", cx),
        Ok(kills) => rows(kills.iter().map(|kill| {
            row(cx)
                .child(cell_flex(kill.target.clone(), true, cx))
                .child(cell(SOURCE_WIDTH, format!("by {}", kill.by), true, cx))
                .child(time(ago(kill.time_us), cx))
        })),
        Err(err) => problem(err, cx),
    };
    let oom_count = diagnostics.oom_kills.as_ref().ok().map(|kills| kills.len().to_string());

    vec![
        section("Failed units", failed_count, failed, cx),
        section("Errors this boot", error_count, errors, cx),
        section("Crashes, last 7 days", crash_count, crashes, cx),
        section("Out-of-memory kills this boot", oom_count, oom, cx),
    ]
}

fn section(title: &'static str, count: Option<String>, body: AnyElement, cx: &App) -> AnyElement {
    v_flex()
        .flex_shrink_0()
        .gap_2()
        .child(
            h_flex()
                .gap_2()
                .items_baseline()
                .child(div().text_sm().font_semibold().child(title))
                .children(count.map(|count| div().text_xs().text_color(cx.theme().muted_foreground).child(count))),
        )
        .child(body)
        .into_any_element()
}

fn rows<E: IntoElement>(rows: impl Iterator<Item = E>) -> AnyElement {
    v_flex().gap_1().children(rows).into_any_element()
}

fn row(cx: &App) -> Div {
    h_flex().gap_3().items_center().text_xs().py_0p5().border_b_1().border_color(cx.theme().border.opacity(0.5))
}

/// A fixed-width column.
fn cell(width: f32, text: impl Into<SharedString>, muted: bool, cx: &App) -> Div {
    let cell = div().w(px(width)).flex_shrink_0().truncate().child(text.into());
    if muted { cell.text_color(cx.theme().muted_foreground) } else { cell }
}

/// A column that takes the space left over.
fn cell_flex(text: impl Into<SharedString>, strong: bool, cx: &App) -> Div {
    let cell = div().flex_1().min_w_0().truncate().child(text.into());
    if strong { cell.font_semibold() } else { cell.text_color(cx.theme().muted_foreground) }
}

fn time(text: String, cx: &App) -> Div {
    div().w(px(TIME_WIDTH)).flex_shrink_0().text_right().text_color(cx.theme().muted_foreground).child(text)
}

/// An empty list: a check mark and a short line, so it doesn't rely on color.
fn all_clear(text: &'static str, cx: &App) -> AnyElement {
    h_flex()
        .gap_2()
        .items_center()
        .text_xs()
        .child(Icon::new(CatalogIcon::CircleCheck).small().text_color(cx.theme().success))
        .child(text)
        .into_any_element()
}

/// The section's tool couldn't be run or read.
fn problem(text: &str, cx: &App) -> AnyElement {
    h_flex()
        .gap_2()
        .items_center()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(Icon::new(CatalogIcon::TriangleAlert).small().text_color(cx.theme().warning))
        .child(text.to_string())
        .into_any_element()
}
