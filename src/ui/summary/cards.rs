//! The row of cards under the memory chart: pressure, network, disk and GPUs.

use gpui_kit::component::StyledExt as _;
use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::panel::{caption, panel};
use crate::system::gpu::GpuReading;
use crate::system::history::History;
use crate::system::monitor::Monitor;
use crate::system::{Snapshot, format};
use crate::ui::charts::{Palette, Series, line_chart, meter};

const CARD_MIN_WIDTH: f32 = 250.;
const CARD_HEIGHT: f32 = 132.;
const SPARKLINE_HEIGHT: f32 = 40.;
const METER_HEIGHT: f32 = 8.;
/// The smallest scale the network and disk sparklines use, in bytes per second.
const NETWORK_FLOOR: f32 = 64. * 1024.;
const DISK_FLOOR: f32 = 1024. * 1024.;

pub fn render(monitor: &Monitor, snapshot: &Snapshot, palette: &Palette, cx: &App) -> impl IntoElement {
    let cards = [pressure(snapshot, cx), network(monitor, snapshot, palette, cx), disk(monitor, snapshot, palette, cx), gpus(snapshot, cx)];
    h_flex()
        .flex_wrap()
        .items_stretch()
        .gap_3()
        .children(cards.into_iter().map(|card| card.flex_1().min_w(px(CARD_MIN_WIDTH)).h(px(CARD_HEIGHT))))
}

fn pressure(snapshot: &Snapshot, cx: &App) -> Div {
    let theme = cx.theme();
    let Some(pressure) = snapshot.pressure else {
        return panel("System pressure", None, cx).child(caption("This kernel doesn't report pressure (PSI)", cx));
    };
    // Spelled out, so the level never depends on color alone.
    let (level, color) = match pressure.highest() {
        p if p < 0.1 => ("Low", theme.success),
        p if p < 0.4 => ("Moderate", theme.warning),
        _ => ("High", theme.danger),
    };
    let row = |label: &'static str, value: f32| {
        h_flex()
            .gap_2()
            .items_center()
            .child(div().w(px(52.)).text_xs().text_color(theme.muted_foreground).child(label))
            .child(div().flex_1().h(px(METER_HEIGHT)).child(meter(value, color)))
            .child(div().w(px(44.)).text_xs().text_right().child(format::percent(value)))
    };
    let trailing = div().text_xs().font_semibold().text_color(color).child(level).into_any_element();
    panel("System pressure", Some(trailing), cx).child(
        v_flex()
            .gap_1p5()
            .child(row("CPU", pressure.cpu))
            .child(row("Memory", pressure.memory))
            .child(row("I/O", pressure.io)),
    )
}

fn network(monitor: &Monitor, snapshot: &Snapshot, palette: &Palette, cx: &App) -> Div {
    let rates = snapshot.network;
    rates_card(
        "Network",
        [("↓", rates.received, palette.received), ("↑", rates.sent, palette.sent)],
        [&monitor.received, &monitor.sent],
        NETWORK_FLOOR,
        None,
        cx,
    )
}

fn disk(monitor: &Monitor, snapshot: &Snapshot, palette: &Palette, cx: &App) -> Div {
    let rates = snapshot.disk;
    let busy = caption(format!("{} busy", format::percent(rates.busy)), cx).into_any_element();
    rates_card(
        "Disk",
        [("Read", rates.read, palette.read), ("Write", rates.written, palette.written)],
        [&monitor.disk_read, &monitor.disk_written],
        DISK_FLOOR,
        Some(busy),
        cx,
    )
}

/// Two rates with a shared-scale sparkline, so the lines compare honestly.
fn rates_card(
    title: &'static str,
    rates: [(&'static str, f64, Hsla); 2],
    histories: [&History; 2],
    floor: f32,
    trailing: Option<AnyElement>,
    cx: &App,
) -> Div {
    let max = histories.iter().flat_map(|history| history.values()).fold(floor, f32::max);
    let series = histories.iter().zip(&rates).map(|(history, (_, _, color))| Series::new(history, max, *color)).collect();
    let readings = rates.iter().map(|&(label, value, color)| {
        h_flex()
            .gap_1()
            .items_center()
            .text_xs()
            .child(div().text_color(color).child(label))
            .child(format::rate(value))
    });
    panel(title, trailing, cx)
        .child(h_flex().gap_4().children(readings))
        .child(div().h(px(SPARKLINE_HEIGHT)).child(line_chart(series, None)))
}

fn gpus(snapshot: &Snapshot, cx: &App) -> Div {
    let count = snapshot.gpus.len();
    let trailing = (count > 1).then(|| caption(format!("{count} GPUs"), cx).into_any_element());
    let card = panel(if count > 1 { "GPUs" } else { "GPU" }, trailing, cx);
    if count == 0 {
        return card.child(caption("No GPUs found", cx));
    }
    card.child(v_flex().gap_2().children(snapshot.gpus.iter().map(|gpu| gpu_row(gpu, cx))))
}

fn gpu_row(gpu: &GpuReading, cx: &App) -> impl IntoElement {
    let theme = cx.theme();
    let mut details = Vec::new();
    if let Some(busy) = gpu.busy {
        details.push(format!("{} busy", format::percent(busy)));
    }
    if let Some(celsius) = gpu.temperature {
        details.push(format::celsius(celsius));
    }
    if let Some(watts) = gpu.watts {
        details.push(format!("{watts:.1} W"));
    }
    let detail = match (gpu.awake, details.is_empty()) {
        (false, _) => "Sleeping to save power".to_string(),
        (true, true) => "Awake · no readings from this driver".to_string(),
        (true, false) => details.join(" · "),
    };
    let (state, state_color) = if gpu.awake { ("Awake", theme.success) } else { ("Sleeping", theme.muted_foreground) };
    v_flex()
        .gap_0p5()
        .child(
            h_flex()
                .gap_2()
                .justify_between()
                .child(div().text_xs().font_medium().min_w_0().truncate().child(gpu.name.clone()))
                .child(div().flex_shrink_0().text_xs().text_color(state_color).child(state)),
        )
        .child(caption(detail, cx))
}
