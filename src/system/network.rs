//! How the machine is connected: default gateways, DNS servers and every
//! interface with its kind, driver and Wi-Fi link. Read on a timer while the
//! System Info Network tab is showing, not every second.

use std::net::{Ipv4Addr, Ipv6Addr};
use std::process::Command;

use super::net_info::{InterfaceInfo, read_addresses};

/// systemd-resolved writes the real upstream servers here. `/etc/resolv.conf`
/// often only names its local stub, `127.0.0.53`.
const RESOLVED_UPSTREAM: &str = "/run/systemd/resolve/resolv.conf";
/// Linux's `ARPHRD_*` link types that matter here.
const ARPHRD_ETHER: u32 = 1;
const ARPHRD_PPP: u32 = 512;
const ARPHRD_LOOPBACK: u32 = 772;
const ARPHRD_NONE: u32 = 65534;
/// The daemons that might be managing the network, by process name.
const SERVICES: [(&str, &str); 7] = [
    ("NetworkManager", "NetworkManager"),
    ("systemd-networkd", "systemd-networkd"),
    ("iwd", "iwd"),
    ("connmand", "ConnMan"),
    ("dhcpcd", "dhcpcd"),
    ("wpa_supplicant", "wpa_supplicant"),
    ("systemd-resolved", "systemd-resolved"),
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkInfo {
    /// The default route's gateway and the interface it goes out of.
    pub gateway_v4: Option<(Ipv4Addr, String)>,
    pub gateway_v6: Option<(Ipv6Addr, String)>,
    pub dns: Vec<String>,
    pub search: Vec<String>,
    /// Physical interfaces first, then virtual ones, each by name. No loopback.
    pub interfaces: Vec<NetInterface>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetInterface {
    pub name: String,
    pub kind: LinkKind,
    /// Backed by real hardware (has a `device` link).
    pub physical: bool,
    pub info: InterfaceInfo,
    pub mtu: Option<u32>,
    pub driver: Option<String>,
    pub wifi: Option<WifiLink>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LinkKind {
    Ethernet,
    WiFi,
    Bridge,
    VirtualEthernet,
    WireGuard,
    Tunnel,
    #[default]
    Virtual,
}

impl LinkKind {
    pub fn label(self) -> &'static str {
        match self {
            LinkKind::Ethernet => "Ethernet",
            LinkKind::WiFi => "Wi-Fi",
            LinkKind::Bridge => "Bridge",
            LinkKind::VirtualEthernet => "Virtual Ethernet",
            LinkKind::WireGuard => "WireGuard",
            LinkKind::Tunnel => "Tunnel",
            LinkKind::Virtual => "Virtual",
        }
    }
}

/// What `iw dev <name> link` says about a connected Wi-Fi link.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WifiLink {
    pub ssid: Option<String>,
    pub frequency_mhz: Option<f32>,
    pub signal_dbm: Option<i32>,
    pub rx_mbps: Option<f32>,
    pub tx_mbps: Option<f32>,
    /// `Wi-Fi 7`, `Wi-Fi 6E` and so on, from the receive rate's MCS type.
    pub standard: Option<&'static str>,
}

impl WifiLink {
    /// `2.4 GHz`, `5 GHz` or `6 GHz`.
    pub fn band(&self) -> Option<&'static str> {
        self.frequency_mhz.map(band)
    }
}

impl NetworkInfo {
    pub fn read() -> Self {
        let route = std::fs::read_to_string("/proc/net/route").unwrap_or_default();
        let route6 = std::fs::read_to_string("/proc/net/ipv6_route").unwrap_or_default();
        let resolv = std::fs::read_to_string(RESOLVED_UPSTREAM)
            .or_else(|_| std::fs::read_to_string("/etc/resolv.conf"))
            .unwrap_or_default();
        let (dns, search) = parse_resolv(&resolv);
        Self {
            gateway_v4: default_gateway_v4(&route),
            gateway_v6: default_gateway_v6(&route6),
            dns,
            search,
            interfaces: read_interfaces(),
        }
    }
}

fn read_interfaces() -> Vec<NetInterface> {
    let Ok(entries) = std::fs::read_dir("/sys/class/net") else {
        return Vec::new();
    };
    let mut addresses = read_addresses();
    let mut interfaces: Vec<NetInterface> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let dir = entry.path();
            let read = |file: &str| std::fs::read_to_string(dir.join(file)).ok();
            let link_type = read("type").and_then(|text| text.trim().parse().ok()).unwrap_or(ARPHRD_ETHER);
            if link_type == ARPHRD_LOOPBACK {
                return None;
            }
            let physical = dir.join("device").exists();
            let devtype = read("uevent").and_then(|uevent| devtype(&uevent));
            let kind = link_kind(&name, link_type, physical, devtype.as_deref(), |file| dir.join(file).exists());
            let info = InterfaceInfo::read(&name, &mut addresses);
            let wifi = (kind == LinkKind::WiFi && info.connected).then(|| read_wifi(&name)).flatten();
            Some(NetInterface {
                kind,
                physical,
                mtu: read("mtu").and_then(|text| text.trim().parse().ok()),
                driver: std::fs::read_link(dir.join("device/driver"))
                    .ok()
                    .and_then(|link| link.file_name().map(|name| name.to_string_lossy().into_owned())),
                info,
                wifi,
                name,
            })
        })
        .collect();
    interfaces.sort_by(|a, b| b.physical.cmp(&a.physical).then_with(|| a.name.cmp(&b.name)));
    interfaces
}

/// `DEVTYPE` from a sysfs uevent file, like `wlan`, `bridge` or `wireguard`.
fn devtype(uevent: &str) -> Option<String> {
    uevent.lines().find_map(|line| line.strip_prefix("DEVTYPE=")).map(str::to_string)
}

/// `has` checks for a file in the interface's sysfs folder.
fn link_kind(name: &str, link_type: u32, physical: bool, devtype: Option<&str>, has: impl Fn(&str) -> bool) -> LinkKind {
    match devtype {
        Some("wlan") => return LinkKind::WiFi,
        Some("bridge") => return LinkKind::Bridge,
        Some("wireguard") => return LinkKind::WireGuard,
        _ => {}
    }
    if has("wireless") {
        LinkKind::WiFi
    } else if has("bridge") {
        LinkKind::Bridge
    } else if has("tun_flags") || link_type == ARPHRD_PPP || link_type == ARPHRD_NONE {
        LinkKind::Tunnel
    } else if physical && link_type == ARPHRD_ETHER {
        LinkKind::Ethernet
    } else if name.starts_with("veth") {
        LinkKind::VirtualEthernet
    } else {
        LinkKind::Virtual
    }
}

/// The lowest-metric IPv4 default route. Addresses in `/proc/net/route` are
/// hex in the kernel's byte order.
fn default_gateway_v4(route: &str) -> Option<(Ipv4Addr, String)> {
    route
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let (iface, destination, gateway, flags, metric, mask) =
                (fields.first()?, fields.get(1)?, fields.get(2)?, fields.get(3)?, fields.get(6)?, fields.get(7)?);
            let flags = u32::from_str_radix(flags, 16).ok()?;
            // RTF_UP and RTF_GATEWAY, to 0.0.0.0/0.
            let default = *destination == "00000000" && *mask == "00000000" && flags & 0x3 == 0x3;
            let gateway = u32::from_str_radix(gateway, 16).ok()?;
            default.then(|| (metric.parse::<u32>().unwrap_or(u32::MAX), Ipv4Addr::from(gateway.to_ne_bytes()), iface.to_string()))
        })
        .min_by_key(|(metric, ..)| *metric)
        .map(|(_, gateway, iface)| (gateway, iface))
}

/// The lowest-metric IPv6 default route with a next hop.
fn default_gateway_v6(route: &str) -> Option<(Ipv6Addr, String)> {
    route
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let (destination, prefix, next_hop, metric, iface) =
                (fields.first()?, fields.get(1)?, fields.get(4)?, fields.get(5)?, fields.get(9)?);
            let next_hop = u128::from_str_radix(next_hop, 16).ok()?;
            let default = destination.bytes().all(|b| b == b'0') && *prefix == "00" && next_hop != 0 && *iface != "lo";
            default.then(|| (u32::from_str_radix(metric, 16).unwrap_or(u32::MAX), Ipv6Addr::from(next_hop), iface.to_string()))
        })
        .min_by_key(|(metric, ..)| *metric)
        .map(|(_, gateway, iface)| (gateway, iface))
}

/// Name servers and search domains. `search .` means none.
fn parse_resolv(text: &str) -> (Vec<String>, Vec<String>) {
    let mut servers = Vec::new();
    let mut search = Vec::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("nameserver") => servers.extend(words.next().map(str::to_string)),
            Some("search" | "domain") => search.extend(words.filter(|domain| *domain != ".").map(str::to_string)),
            _ => {}
        }
    }
    search.dedup();
    (servers, search)
}

/// Runs `iw` for a connected Wi-Fi interface. `None` when `iw` isn't installed.
fn read_wifi(name: &str) -> Option<WifiLink> {
    let output = Command::new("iw").args(["dev", name, "link"]).output().ok()?;
    output.status.success().then(|| parse_iw_link(&String::from_utf8_lossy(&output.stdout))).flatten()
}

fn parse_iw_link(text: &str) -> Option<WifiLink> {
    if !text.starts_with("Connected to") {
        return None;
    }
    let mut link = WifiLink::default();
    let rate = |value: &str| value.split_whitespace().next()?.parse().ok();
    for line in text.lines().map(str::trim) {
        let Some((key, value)) = line.split_once(": ") else {
            continue;
        };
        match key {
            "SSID" => link.ssid = Some(value.to_string()),
            "freq" => link.frequency_mhz = value.parse().ok(),
            "signal" => link.signal_dbm = value.split_whitespace().next().and_then(|dbm| dbm.parse().ok()),
            "rx bitrate" => {
                link.rx_mbps = rate(value);
                link.standard = standard(value, link.frequency_mhz);
            }
            "tx bitrate" => link.tx_mbps = rate(value),
            _ => {}
        }
    }
    Some(link)
}

fn band(mhz: f32) -> &'static str {
    if mhz < 3000. {
        "2.4 GHz"
    } else if mhz < 5925. {
        "5 GHz"
    } else {
        "6 GHz"
    }
}

/// The Wi-Fi generation from a bitrate line's MCS type: EHT is 802.11be, HE
/// is 802.11ax, VHT is 802.11ac, and plain MCS is 802.11n.
fn standard(bitrate: &str, frequency_mhz: Option<f32>) -> Option<&'static str> {
    let six_ghz = frequency_mhz.is_some_and(|mhz| mhz >= 5925.);
    bitrate.split_whitespace().find_map(|token| {
        if token.starts_with("EHT-MCS") {
            Some("Wi-Fi 7")
        } else if token.starts_with("HE-MCS") {
            Some(if six_ghz { "Wi-Fi 6E" } else { "Wi-Fi 6" })
        } else if token.starts_with("VHT-MCS") {
            Some("Wi-Fi 5")
        } else if token == "MCS" {
            Some("Wi-Fi 4")
        } else {
            None
        }
    })
}

/// The network daemons among the running process names, in a fixed order.
pub fn network_services<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<&'static str> {
    let running: Vec<&str> = names.into_iter().collect();
    SERVICES.iter().filter(|(process, _)| running.contains(process)).map(|(_, label)| *label).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Stands in for an interface's sysfs folder holding just these files.
    fn sysfs_has<'a>(files: &'a [&'a str]) -> impl Fn(&str) -> bool + 'a {
        move |file| files.contains(&file)
    }

    const ROUTE: &str = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
        wlan0\t00000000\t0101A8C0\t0003\t0\t0\t600\t00000000\t0\t0\t0\n\
        enp1s0\t00000000\t0102A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
        docker0\t000011AC\t00000000\t0001\t0\t0\t0\t0000FFFF\t0\t0\t0\n";

    #[test]
    #[cfg(target_endian = "little")]
    fn ipv4_gateway_takes_the_lowest_metric() {
        assert_eq!(default_gateway_v4(ROUTE), Some((Ipv4Addr::new(192, 168, 2, 1), "enp1s0".into())));
        assert_eq!(default_gateway_v4("Iface\tDestination\n"), None);
    }

    #[test]
    fn ipv6_gateway_needs_a_next_hop() {
        let route = "00000000000000000000000000000000 00 00000000000000000000000000000000 00 fe80000000000000eacd15fffe96eba7 00000258 00000021 00000000 00000003    wlan0\n\
            00000000000000000000000000000000 00 00000000000000000000000000000000 00 00000000000000000000000000000000 ffffffff 00000001 00000000 00200200       lo\n";
        let expected: Ipv6Addr = "fe80::eacd:15ff:fe96:eba7".parse().unwrap();
        assert_eq!(default_gateway_v6(route), Some((expected, "wlan0".into())));
    }

    #[test]
    fn resolv_servers_and_search() {
        let text = "# comment\nnameserver 75.75.75.75\nnameserver 2001:558:feed::1\nsearch .\noptions edns0\n";
        assert_eq!(parse_resolv(text), (vec!["75.75.75.75".into(), "2001:558:feed::1".into()], vec![]));
        assert_eq!(parse_resolv("search lan home.arpa\n").1, vec!["lan".to_string(), "home.arpa".to_string()]);
    }

    #[test]
    fn link_kinds() {
        let none = sysfs_has(&[]);
        assert_eq!(link_kind("wlan0", ARPHRD_ETHER, true, Some("wlan"), &none), LinkKind::WiFi);
        assert_eq!(link_kind("enp109s0", ARPHRD_ETHER, true, None, &none), LinkKind::Ethernet);
        assert_eq!(link_kind("docker0", ARPHRD_ETHER, false, Some("bridge"), &none), LinkKind::Bridge);
        assert_eq!(link_kind("veth616a1d4", ARPHRD_ETHER, false, None, &none), LinkKind::VirtualEthernet);
        assert_eq!(link_kind("wg0", ARPHRD_NONE, false, Some("wireguard"), &none), LinkKind::WireGuard);
        assert_eq!(link_kind("tun0", ARPHRD_NONE, false, None, sysfs_has(&["tun_flags"])), LinkKind::Tunnel);
        assert_eq!(link_kind("dummy0", ARPHRD_ETHER, false, None, &none), LinkKind::Virtual);
    }

    #[test]
    fn iw_link_output() {
        let text = "Connected to e8:cd:15:a5:eb:ab (on wlan0)\n\tSSID: The Bit Bucket X\n\tfreq: 6935.0\n\tsignal: -64 dBm\n\
            \trx bitrate: 2305.6 MBit/s 320MHz EHT-MCS 5 EHT-NSS 2 EHT-GI 0\n\ttx bitrate: 77.4 MBit/s EHT-MCS 6 EHT-NSS 1 EHT-GI 0\n";
        let link = parse_iw_link(text).unwrap();
        assert_eq!(link.ssid.as_deref(), Some("The Bit Bucket X"));
        assert_eq!(link.band(), Some("6 GHz"));
        assert_eq!(link.signal_dbm, Some(-64));
        assert_eq!(link.rx_mbps, Some(2305.6));
        assert_eq!(link.tx_mbps, Some(77.4));
        assert_eq!(link.standard, Some("Wi-Fi 7"));
        assert_eq!(parse_iw_link("Not connected.\n"), None);
    }

    #[test]
    fn wifi_standards() {
        assert_eq!(standard("1201.0 MBit/s 80MHz HE-MCS 11 HE-NSS 2", Some(5180.)), Some("Wi-Fi 6"));
        assert_eq!(standard("1201.0 MBit/s 160MHz HE-MCS 11 HE-NSS 2", Some(6115.)), Some("Wi-Fi 6E"));
        assert_eq!(standard("866.7 MBit/s VHT-MCS 9 80MHz", Some(5180.)), Some("Wi-Fi 5"));
        assert_eq!(standard("144.4 MBit/s MCS 15 short GI", Some(2437.)), Some("Wi-Fi 4"));
        assert_eq!(standard("54.0 MBit/s", Some(2437.)), None);
    }

    #[test]
    fn reads_this_machine() {
        let info = NetworkInfo::read();
        assert!(info.interfaces.iter().all(|interface| interface.name != "lo"));
        let physical = info.interfaces.iter().take_while(|interface| interface.physical).count();
        assert!(info.interfaces[physical..].iter().all(|interface| !interface.physical));
    }

    #[test]
    fn services_from_process_names() {
        let names = ["zsh", "systemd-resolved", "NetworkManager", "kitty"];
        assert_eq!(network_services(names), vec!["NetworkManager", "systemd-resolved"]);
    }
}
