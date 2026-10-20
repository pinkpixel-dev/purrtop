//! The GPU detail: whatever the driver shares, read only while the GPU is awake.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::devices::{THERMAL_SCALE, gpu_headline};
use super::parts::{facts, header, stats};
use crate::system::gpu::{GpuReading, Vendor};
use crate::system::monitor::GpuHistory;
use crate::system::format;
use crate::ui::charts::{Grid, Palette, Series, line_chart};
use crate::ui::summary::panel::{caption, legend};

/// Vertical space taken by everything but the chart: title and status bars,
/// page heading, the header, legend, stats, facts, padding and gaps.
const RESERVED_HEIGHT: f32 = 440.;
const MIN_CHART_HEIGHT: f32 = 200.;

pub fn render(gpu: &GpuReading, history: &GpuHistory, palette: &Palette, viewport: Size<Pixels>, cx: &App) -> Div {
    let chart_height = (f32::from(viewport.height) - RESERVED_HEIGHT).max(MIN_CHART_HEIGHT);
    let fraction = gpu.busy.or_else(|| gpu.clock_mhz.zip(gpu.max_clock_mhz).map(|(mhz, max)| mhz / max)).unwrap_or(0.);
    let mut subtitle = gpu.vendor.label().to_string();
    if let Some(driver) = &gpu.driver {
        subtitle.push_str(&format!(" · {driver}"));
    }

    // Only lines for readings this driver gives.
    let mut series = Vec::new();
    let mut legends = Vec::new();
    if gpu.temperature.is_some() {
        series.push(Series::new(&history.temperature, THERMAL_SCALE, palette.temperature));
        legends.push(legend("Temperature", palette.temperature, cx));
    }
    if let Some(max) = gpu.max_clock_mhz {
        series.push(Series::new(&history.clock, max, palette.clock));
        legends.push(legend("Clock", palette.clock, cx));
    }
    if gpu.busy.is_some() {
        series.push(Series::new(&history.busy, 1., palette.gpu).area());
        legends.push(legend("Usage", palette.gpu, cx));
    }
    let note = match (gpu.awake, series.is_empty(), gpu.vendor) {
        (false, _, _) => Some("Sleeping. purrtop doesn't read a sleeping GPU, so it stays asleep."),
        (true, true, Vendor::Nvidia) => Some("Turn on NVIDIA details in Settings (Ctrl+,) to see usage."),
        (true, true, _) => Some("This driver doesn't share any readings."),
        (true, false, _) => None,
    };

    let clock = match (gpu.clock_mhz, gpu.max_clock_mhz) {
        (Some(mhz), Some(max)) => format!("{} / {}", format::frequency(mhz), format::frequency(max)),
        (Some(mhz), None) => format::frequency(mhz),
        _ => "n/a".into(),
    };
    let vram = match (gpu.vram_used, gpu.vram_total) {
        (Some(used), Some(total)) => format!("{} / {}", format::bytes(used), format::bytes(total)),
        _ => "n/a".into(),
    };

    v_flex()
        .gap_3()
        .child(header(gpu.name.clone(), subtitle, fraction, gpu_headline(gpu), palette.gpu, cx))
        .children(note.map(|note| caption(note, cx)))
        .when(!series.is_empty() && gpu.awake, |this| {
            this.child(h_flex().gap_4().children(legends)).child(
                div()
                    .h(px(chart_height))
                    .flex_shrink_0()
                    .rounded(cx.theme().radius)
                    .border_1()
                    .border_color(cx.theme().border)
                    .overflow_hidden()
                    .child(line_chart(series, Some(Grid { rows: 10, columns: 24, color: palette.grid }))),
            )
        })
        .child(stats(
            vec![
                ("State", if gpu.awake { "Awake" } else { "Sleeping" }.into()),
                ("Usage", gpu.busy.map_or("n/a".into(), format::percent)),
                ("Clock", clock),
                ("Temperature", gpu.temperature.map_or("n/a".into(), format::celsius)),
                ("Power", gpu.watts.map_or("n/a".into(), |watts| format!("{watts:.1} W"))),
                ("Video memory", vram),
            ],
            cx,
        ))
        .child(facts(
            vec![
                ("Vendor", gpu.vendor.label().into()),
                ("Driver", gpu.driver.clone().unwrap_or_else(|| "n/a".into())),
                ("PCI slot", gpu.slot.clone()),
                ("Power management", gpu.power_state.clone().unwrap_or_else(|| "n/a".into())),
            ],
            cx,
        ))
}
