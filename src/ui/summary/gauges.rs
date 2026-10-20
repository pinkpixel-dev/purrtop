//! LED columns for CPU, clock, temperature and GPU.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::panel::{caption, panel};
use crate::system::{Snapshot, format};
use crate::ui::charts::{Palette, led_column};

/// The top of the temperature column, in °C.
const TEMPERATURE_SCALE: f32 = 110.;
const COLUMN_WIDTH: f32 = 14.;

pub fn render(snapshot: &Snapshot, palette: &Palette, cx: &App) -> Div {
    let clock = snapshot.clock;
    let gpu = snapshot.gpus.iter().find(|gpu| gpu.awake && gpu.busy.is_some());

    let columns = [
        column("CPU", snapshot.cpu, format::percent(snapshot.cpu), palette.cpu, cx),
        column(
            "Clock",
            clock.map_or(0., |clock| clock.fraction()),
            clock.map_or("n/a".into(), |clock| format!("{:.1}G", clock.current_mhz / 1000.)),
            palette.clock,
            cx,
        ),
        column(
            "Temp",
            snapshot.temperature.map_or(0., |celsius| celsius / TEMPERATURE_SCALE),
            snapshot.temperature.map_or("n/a".into(), |celsius| format!("{celsius:.0}°")),
            palette.temperature,
            cx,
        ),
        column(
            "GPU",
            gpu.and_then(|gpu| gpu.busy).unwrap_or(0.),
            gpu.and_then(|gpu| gpu.busy).map_or("n/a".into(), format::percent),
            palette.gpu,
            cx,
        ),
    ];

    let trailing = clock.map(|clock| caption(format::frequency(clock.current_mhz), cx).into_any_element());
    panel("System", trailing, cx).child(h_flex().flex_1().min_h_0().gap_2().justify_between().children(columns))
}

fn column(label: &'static str, fraction: f32, value: String, color: Hsla, cx: &App) -> impl IntoElement {
    v_flex()
        .h_full()
        .flex_1()
        .items_center()
        .gap_1()
        .child(div().text_xs().text_color(cx.theme().muted_foreground).child(label))
        .child(div().flex_1().min_h_0().w(px(COLUMN_WIDTH)).child(led_column(fraction, color)))
        .child(div().text_xs().text_color(color).child(value))
}
