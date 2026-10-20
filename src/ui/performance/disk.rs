//! The Disk detail: active time, read and write speed, and the volumes on it.

use gpui_kit::component::{ActiveTheme as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::*;

use super::devices::disk_label;
use super::parts::{DISK_FLOOR, facts, header, shared_scale, stats};
use crate::system::disk_info::Volume;
use crate::system::io::Disk;
use crate::system::monitor::DiskHistory;
use crate::system::{Snapshot, format};
use crate::ui::charts::{Grid, Palette, Series, line_chart, meter};
use crate::ui::summary::panel::{caption, legend};

/// Vertical space taken by everything but the two charts: title and status
/// bars, page heading, the header, legends, stats, facts, padding and gaps.
const RESERVED_HEIGHT: f32 = 520.;
const MIN_CHARTS_HEIGHT: f32 = 240.;
/// The active time chart's share of the chart space.
const ACTIVE_SHARE: f32 = 0.55;
const VOLUME_METER_HEIGHT: f32 = 8.;
const VOLUME_METER_WIDTH: f32 = 160.;

pub fn render(disk: &Disk, history: &DiskHistory, snapshot: &Snapshot, palette: &Palette, viewport: Size<Pixels>, cx: &App) -> Div {
    let rates = disk.rates;
    let info = &disk.info;
    let charts = (f32::from(viewport.height) - RESERVED_HEIGHT).max(MIN_CHARTS_HEIGHT);
    let scale = shared_scale([&history.read, &history.written], DISK_FLOOR);

    let mut subtitle = vec![format::bytes(info.size)];
    if let Some(model) = &info.model {
        subtitle.insert(0, model.clone());
    }

    v_flex()
        .gap_3()
        .child(header(disk_label(disk, snapshot), subtitle.join(" · "), rates.busy, format::percent(rates.busy), palette.read, cx))
        .child(legend("Active time", palette.read, cx))
        .child(chart(vec![Series::new(&history.busy, 1., palette.read).area()], charts * ACTIVE_SHARE, palette, cx))
        .child(
            h_flex()
                .gap_4()
                .child(legend("Read", palette.read, cx))
                .child(legend("Write", palette.written, cx))
                .child(div().flex_1())
                .child(caption(format!("Scale {}", format::rate(f64::from(scale))), cx)),
        )
        .child(chart(
            vec![Series::new(&history.written, scale, palette.written), Series::new(&history.read, scale, palette.read)],
            charts * (1. - ACTIVE_SHARE),
            palette,
            cx,
        ))
        .child(stats(
            vec![
                ("Active time", format::percent(rates.busy)),
                ("Response time", rates.response_ms.map_or("n/a".into(), format::milliseconds)),
                ("Read speed", format::rate(rates.read)),
                ("Write speed", format::rate(rates.written)),
                ("Capacity", format::bytes(info.size)),
            ],
            cx,
        ))
        .child(facts(
            vec![
                ("Model", info.model.clone().unwrap_or_else(|| "n/a".into())),
                ("Type", info.kind.label().into()),
                ("Device", format!("/dev/{}", disk.name)),
                ("Removable", if info.removable { "Yes" } else { "No" }.into()),
            ],
            cx,
        ))
        .child(volumes(&disk.volumes, palette, cx))
}

fn chart(series: Vec<Series>, height: f32, palette: &Palette, cx: &App) -> Div {
    div()
        .h(px(height))
        .flex_shrink_0()
        .rounded(cx.theme().radius)
        .border_1()
        .border_color(cx.theme().border)
        .overflow_hidden()
        .child(line_chart(series, Some(Grid { rows: 5, columns: 24, color: palette.grid })))
}

/// Each mounted filesystem with how full it is.
fn volumes(volumes: &[Volume], palette: &Palette, cx: &App) -> Div {
    let theme = cx.theme();
    let list = v_flex().flex_shrink_0().gap_2().child(div().text_sm().font_semibold().child("Volumes"));
    if volumes.is_empty() {
        return list.child(caption("Nothing on this disk is mounted", cx));
    }
    list.children(volumes.iter().map(|volume| {
        h_flex()
            .gap_3()
            .items_center()
            .text_xs()
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_sm().truncate().child(volume.mount.clone()))
                    .child(div().text_color(theme.muted_foreground).truncate().child(format!("{} · {}", volume.device, volume.filesystem))),
            )
            .child(div().w(px(VOLUME_METER_WIDTH)).flex_shrink_0().h(px(VOLUME_METER_HEIGHT)).child(meter(volume.used_fraction(), palette.read)))
            .child(
                div()
                    .flex_shrink_0()
                    .text_right()
                    .child(format!("{} free of {}", format::bytes(volume.available), format::bytes(volume.total))),
            )
    }))
}
