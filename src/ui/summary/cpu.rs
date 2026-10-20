//! The big CPU history chart: utilization, kernel time and temperature.

use gpui_kit::component::StyledExt as _;
use gpui_kit::component::h_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::panel::{caption, legend, panel};
use crate::system::monitor::Monitor;
use crate::system::{Snapshot, format};
use crate::ui::charts::{Grid, Palette, Series, line_chart};

/// The top of the temperature line, in °C.
const TEMPERATURE_SCALE: f32 = 110.;

pub fn render(monitor: &Monitor, snapshot: &Snapshot, palette: &Palette, cx: &App) -> Div {
    let has_temperature = snapshot.temperature.is_some();
    let mut series = Vec::new();
    if has_temperature {
        series.push(Series::new(&monitor.temperature, TEMPERATURE_SCALE, palette.temperature));
    }
    series.push(Series::new(&monitor.kernel, 1., palette.kernel));
    series.push(Series::new(&monitor.cpu, 1., palette.cpu).area());

    let percent = div().text_lg().font_semibold().text_color(palette.cpu).child(format::percent(snapshot.cpu));
    let [load1, load5, load15] = snapshot.load;
    let mut details = vec![format!("{} logical processors", snapshot.cores.len())];
    if let Some(clock) = snapshot.clock {
        details.push(format::frequency(clock.current_mhz));
    }
    if let Some(celsius) = snapshot.temperature {
        details.push(format::celsius(celsius));
    }

    panel("CPU", Some(percent.into_any_element()), cx)
        .child(
            h_flex()
                .gap_4()
                .child(legend("Utilization", palette.cpu, cx))
                .child(legend("Kernel", palette.kernel, cx))
                .when(has_temperature, |this| this.child(legend("Temperature", palette.temperature, cx))),
        )
        .child(div().flex_1().min_h_0().child(line_chart(series, Some(Grid { rows: 10, columns: 24, color: palette.grid }))))
        .child(
            h_flex()
                .justify_between()
                .gap_3()
                .child(caption(details.join(" · "), cx))
                .child(caption(format!("Load {load1:.2} · {load5:.2} · {load15:.2}"), cx)),
        )
}
