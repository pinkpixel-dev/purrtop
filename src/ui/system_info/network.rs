//! The Network tab: gateways, DNS and the services running the network, then
//! every interface, physical ones first.

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::*;

use super::{device_section, section};
use crate::system::network::{NetInterface, NetworkInfo, network_services};
use crate::system::{Snapshot, format};

pub fn render(info: Option<&NetworkInfo>, snapshot: Option<&Snapshot>, cx: &App) -> Vec<AnyElement> {
    let Some(info) = info else {
        let note = div().text_sm().text_color(cx.theme().muted_foreground).child("Reading network info…");
        return vec![note.into_any_element()];
    };
    let services = snapshot
        .map(|snapshot| network_services(snapshot.processes.iter().map(|process| process.name.as_str())))
        .unwrap_or_default();
    let list = |items: &[String]| (!items.is_empty()).then(|| items.join(", "));
    let mut sections = vec![section(
        "General",
        vec![
            ("Default gateway", info.gateway_v4.as_ref().map(|(gateway, iface)| format!("{gateway} ({iface})"))),
            ("IPv6 gateway", info.gateway_v6.as_ref().map(|(gateway, iface)| format!("{gateway} ({iface})"))),
            ("DNS servers", list(&info.dns)),
            ("Search domains", list(&info.search)),
            ("Network services", (!services.is_empty()).then(|| services.join(", "))),
        ],
        cx,
    )];
    if !info.interfaces.is_empty() {
        let devices = info.interfaces.iter().map(|interface| (format!("{} · {}", interface.name, interface.kind.label()), facts(interface))).collect();
        sections.push(device_section("Interfaces", devices, cx));
    }
    sections
}

fn facts(interface: &NetInterface) -> super::Facts {
    let info = &interface.info;
    let wifi = interface.wifi.as_ref();
    let mbps = |rate: f32| format::link_speed(rate.round() as u32);
    let join = |addresses: Vec<String>| (!addresses.is_empty()).then(|| addresses.join(", "));
    vec![
        ("Status", Some(if info.connected { "Connected" } else { "Disconnected" }.into())),
        ("Network", wifi.and_then(|wifi| wifi.ssid.clone())),
        ("Standard", wifi.and_then(|wifi| wifi.standard.map(str::to_string))),
        ("Band", wifi.and_then(|wifi| wifi.band().map(str::to_string))),
        ("Signal", wifi.and_then(|wifi| wifi.signal_dbm.map(|dbm| format!("{dbm} dBm")))),
        ("Receive rate", wifi.and_then(|wifi| wifi.rx_mbps.map(mbps))),
        ("Send rate", wifi.and_then(|wifi| wifi.tx_mbps.map(mbps))),
        ("Link speed", info.speed_mbps.map(format::link_speed)),
        ("MAC address", info.mac.clone()),
        ("MTU", interface.mtu.map(|mtu| mtu.to_string())),
        ("Driver", interface.driver.clone()),
        ("IPv4", join(info.ipv4.iter().map(ToString::to_string).collect())),
        ("IPv6", join(info.ipv6.iter().map(ToString::to_string).collect())),
    ]
}
