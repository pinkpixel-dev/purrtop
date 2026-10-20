//! The Network detail: receive and send over time, totals, and link details.

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::*;

use super::devices::interface_label;
use super::parts::{NETWORK_FLOOR, facts, header, shared_scale, stats};
use crate::system::io::Interface;
use crate::system::monitor::NetHistory;
use crate::system::{Snapshot, format};
use crate::ui::charts::{Grid, Palette, Series, line_chart};
use crate::ui::summary::panel::{caption, legend};

/// Vertical space taken by everything but the chart: title and status bars,
/// page heading, the header, legend, stats, facts, padding and gaps.
const RESERVED_HEIGHT: f32 = 440.;
const MIN_CHART_HEIGHT: f32 = 200.;

pub fn render(interface: &Interface, history: &NetHistory, snapshot: &Snapshot, palette: &Palette, viewport: Size<Pixels>, cx: &App) -> Div {
    let info = &interface.info;
    let rates = interface.rates;
    let scale = shared_scale([&history.received, &history.sent], NETWORK_FLOOR);
    let total = rates.received + rates.sent;
    let chart_height = (f32::from(viewport.height) - RESERVED_HEIGHT).max(MIN_CHART_HEIGHT);
    let state = if info.connected { "Connected" } else { "Not connected" };
    let join = |addresses: Vec<String>| if addresses.is_empty() { "None".to_string() } else { addresses.join(", ") };

    v_flex()
        .gap_3()
        .child(header(
            interface_label(interface, snapshot),
            format!("{} · {state}", interface.name),
            (total / f64::from(scale)) as f32,
            format::rate(total),
            palette.received,
            cx,
        ))
        .child(
            h_flex()
                .gap_4()
                .child(legend("Receive", palette.received, cx))
                .child(legend("Send", palette.sent, cx))
                .child(div().flex_1())
                .child(caption(format!("Scale {}", format::rate(f64::from(scale))), cx)),
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
                    vec![Series::new(&history.sent, scale, palette.sent), Series::new(&history.received, scale, palette.received).area()],
                    Some(Grid { rows: 10, columns: 24, color: palette.grid }),
                )),
        )
        .child(stats(
            vec![
                ("Receive", format::rate(rates.received)),
                ("Send", format::rate(rates.sent)),
                ("Received since boot", format::bytes(interface.total_received)),
                ("Sent since boot", format::bytes(interface.total_sent)),
            ],
            cx,
        ))
        .child(facts(
            vec![
                ("Interface", interface.name.clone()),
                ("Type", if info.wireless { "Wi-Fi" } else { "Ethernet" }.into()),
                ("State", state.into()),
                ("Link speed", info.speed_mbps.map_or("n/a".into(), format::link_speed)),
                ("MAC address", info.mac.clone().unwrap_or_else(|| "n/a".into())),
                ("IPv4", join(info.ipv4.iter().map(ToString::to_string).collect())),
                ("IPv6", join(info.ipv6.iter().map(ToString::to_string).collect())),
            ],
            cx,
        ))
}
