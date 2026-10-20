//! The Thermals detail: every sensor on one chart, then a row per sensor.

use gpui_kit::component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::*;

use super::devices::THERMAL_SCALE;
use super::parts::header;
use crate::system::monitor::Monitor;
use crate::system::thermals::Temperature;
use crate::system::{Snapshot, format};
use crate::ui::charts::{Grid, Palette, Series, line_chart, meter};

/// Vertical space taken by everything but the chart: title and status bars,
/// page heading, the header, padding, and room for a few sensor rows.
const RESERVED_HEIGHT: f32 = 480.;
const MIN_CHART_HEIGHT: f32 = 200.;
const METER_WIDTH: f32 = 160.;
const METER_HEIGHT: f32 = 8.;
const VALUE_WIDTH: f32 = 56.;
const PEAK_WIDTH: f32 = 96.;
/// How close to the hardware's limit a sensor gets before it's flagged.
const NEAR_LIMIT: f32 = 10.;

pub fn render(monitor: &Monitor, snapshot: &Snapshot, palette: &Palette, viewport: Size<Pixels>, cx: &App) -> Div {
    let readings = &snapshot.thermals;
    let hottest = readings.iter().max_by(|a, b| a.celsius.total_cmp(&b.celsius));
    let chart_height = (f32::from(viewport.height) - RESERVED_HEIGHT).max(MIN_CHART_HEIGHT);

    // Drawn in reverse so the first sensors (the CPU) end up on top.
    let series: Vec<Series> = readings
        .iter()
        .enumerate()
        .rev()
        .filter_map(|(ix, reading)| Some(Series::new(monitor.thermals.get(&reading.key)?, THERMAL_SCALE, palette.cycled(ix))))
        .collect();

    v_flex()
        .gap_3()
        .child(header(
            "Thermals",
            hottest.map_or_else(String::new, |reading| format!("Hottest: {}", reading.label)),
            hottest.map_or(0., |reading| reading.celsius / THERMAL_SCALE),
            hottest.map_or("n/a".into(), |reading| format::celsius(reading.celsius)),
            palette.temperature,
            cx,
        ))
        .child(
            div()
                .h(px(chart_height))
                .flex_shrink_0()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .overflow_hidden()
                // One grid row per 10 °C.
                .child(line_chart(series, Some(Grid { rows: 11, columns: 24, color: palette.grid }))),
        )
        .child(v_flex().flex_shrink_0().gap_2().children(readings.iter().enumerate().map(|(ix, reading)| {
            let peak = monitor.thermals.get(&reading.key).and_then(|history| history.values().reduce(f32::max));
            sensor_row(reading, peak, palette.cycled(ix), palette, cx)
        })))
}

fn sensor_row(reading: &Temperature, peak: Option<f32>, color: Hsla, palette: &Palette, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let top = reading.critical.unwrap_or(THERMAL_SCALE);
    let near_limit = reading.critical.is_some_and(|critical| reading.celsius >= critical - NEAR_LIMIT);
    let mut detail = Vec::new();
    if let Some(critical) = reading.critical {
        detail.push(format!("Limit {}", format::celsius(critical)));
    }
    if near_limit {
        detail.push("Near its limit".into());
    }
    h_flex()
        .gap_3()
        .items_center()
        .text_xs()
        .child(div().size_2().flex_shrink_0().rounded_full().bg(color))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().text_sm().truncate().child(reading.label.clone()))
                .child(div().text_color(if near_limit { theme.warning } else { theme.muted_foreground }).truncate().child(detail.join(" · "))),
        )
        .child(
            div()
                .w(px(METER_WIDTH))
                .flex_shrink_0()
                .h(px(METER_HEIGHT))
                .child(meter(reading.celsius / top, if near_limit { theme.warning } else { palette.temperature })),
        )
        .child(div().w(px(VALUE_WIDTH)).flex_shrink_0().text_right().font_semibold().child(format::celsius(reading.celsius)))
        .child(
            div()
                .w(px(PEAK_WIDTH))
                .flex_shrink_0()
                .text_right()
                .text_color(theme.muted_foreground)
                .child(peak.map_or_else(String::new, |peak| format!("Peak {}", format::celsius(peak)))),
        )
}
