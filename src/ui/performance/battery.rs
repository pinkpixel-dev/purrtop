//! The Battery detail: charge and power over time, health and capacity.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::devices::battery_label;
use super::parts::{facts, header, shared_scale, stats};
use crate::system::battery::Battery;
use crate::system::monitor::BatteryHistory;
use crate::system::{Snapshot, format};
use crate::ui::charts::{Grid, Palette, Series, line_chart};
use crate::ui::summary::panel::{caption, legend};

/// Vertical space taken by everything but the chart: title and status bars,
/// page heading, the header, legend, stats, facts, padding and gaps.
const RESERVED_HEIGHT: f32 = 440.;
const MIN_CHART_HEIGHT: f32 = 200.;
/// The smallest power scale, in watts, so idle draw doesn't fill the chart.
const POWER_FLOOR: f32 = 20.;

pub fn render(battery: &Battery, history: &BatteryHistory, snapshot: &Snapshot, palette: &Palette, viewport: Size<Pixels>, cx: &App) -> Div {
    let chart_height = (f32::from(viewport.height) - RESERVED_HEIGHT).max(MIN_CHART_HEIGHT);
    let power_scale = shared_scale([&history.watts], POWER_FLOOR);
    let name: Vec<&str> = [battery.manufacturer.as_deref(), battery.model.as_deref()].into_iter().flatten().collect();
    let wh = |wh: Option<f32>| wh.map_or("n/a".into(), |wh| format!("{wh:.1} Wh"));

    v_flex()
        .gap_3()
        .child(header(
            battery_label(battery, snapshot),
            if name.is_empty() { battery.name.clone() } else { name.join(" ") },
            battery.charge,
            format::percent(battery.charge),
            palette.battery,
            cx,
        ))
        .child(
            h_flex()
                .gap_4()
                .child(legend("Charge", palette.battery, cx))
                .child(legend("Power", palette.power, cx))
                .child(div().flex_1())
                .child(caption(format!("Power scale {power_scale:.0} W"), cx)),
        )
        .child(
            div()
                .h(px(chart_height))
                .flex_shrink_0()
                .rounded(cx.theme().radius)
                .border_1()
                .border_color(cx.theme().border)
                .overflow_hidden()
                .child(line_chart(
                    vec![Series::new(&history.watts, power_scale, palette.power), Series::new(&history.charge, 1., palette.battery).area()],
                    Some(Grid { rows: 10, columns: 24, color: palette.grid }),
                )),
        )
        .child(stats(
            vec![
                ("Charge", format::percent(battery.charge)),
                ("State", battery.state.label().into()),
                ("Power", battery.watts.map_or("n/a".into(), |watts| format!("{watts:.1} W"))),
                ("Time left", battery.time_left.map_or("n/a".into(), format::duration)),
                ("Health", battery.health.map_or("n/a".into(), format::percent)),
                ("Cycles", battery.cycles.map_or("n/a".into(), |cycles| cycles.to_string())),
            ],
            cx,
        ))
        .child(facts(
            vec![
                ("Power source", if snapshot.plugged_in { "AC adapter" } else { "Battery" }.into()),
                ("Manufacturer", battery.manufacturer.clone().unwrap_or_else(|| "n/a".into())),
                ("Model", battery.model.clone().unwrap_or_else(|| "n/a".into())),
                ("Technology", battery.technology.clone().unwrap_or_else(|| "n/a".into())),
                ("Full capacity", wh(battery.full_wh)),
                ("Design capacity", wh(battery.design_wh)),
            ],
            cx,
        ))
}
