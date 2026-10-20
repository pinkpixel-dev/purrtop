//! Network and disk counters per device, turned into per-second rates between readings.

use std::path::Path;
use std::time::Duration;

use super::disk_info::{DiskInfo, Volume};
use super::net_info::InterfaceInfo;

/// Bytes received and sent by one physical network interface since boot.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetCounters {
    pub name: String,
    pub received: u64,
    pub sent: u64,
}

impl NetCounters {
    /// Every physical interface, in `/proc/net/dev` order.
    pub fn read() -> Option<Vec<Self>> {
        let text = std::fs::read_to_string("/proc/net/dev").ok()?;
        Some(Self::parse(&text, |name| Path::new("/sys/class/net").join(name).join("device").exists()))
    }

    /// Keeps the interfaces `physical` accepts. Loopback, bridges, Docker and
    /// VPN tunnels have no `device` link in sysfs, so they're left out.
    fn parse(text: &str, physical: impl Fn(&str) -> bool) -> Vec<Self> {
        let mut interfaces = Vec::new();
        for line in text.lines().skip(2) {
            let Some((name, fields)) = line.split_once(':') else {
                continue;
            };
            let name = name.trim();
            if !physical(name) {
                continue;
            }
            let fields: Vec<u64> = fields.split_whitespace().filter_map(|field| field.parse().ok()).collect();
            interfaces.push(Self {
                name: name.to_string(),
                received: fields.first().copied().unwrap_or(0),
                sent: fields.get(8).copied().unwrap_or(0),
            });
        }
        interfaces
    }
}

/// Cumulative activity for one whole disk.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiskCounters {
    pub name: String,
    pub read: u64,
    pub written: u64,
    /// Milliseconds the disk spent doing I/O.
    pub busy_ms: u64,
    /// Reads and writes completed, and the milliseconds they took.
    pub requests: u64,
    pub request_ms: u64,
}

/// Sector size the kernel uses in `/proc/diskstats`, whatever the hardware's.
const SECTOR: u64 = 512;

impl DiskCounters {
    /// Every whole disk, in `/proc/diskstats` order.
    pub fn read() -> Option<Vec<Self>> {
        let text = std::fs::read_to_string("/proc/diskstats").ok()?;
        Some(Self::parse(&text, |name| Path::new("/sys/block").join(name).join("device").exists()))
    }

    /// Keeps the devices `whole_disk` accepts. Partitions aren't in
    /// `/sys/block`, and loop, zram and device-mapper entries have no `device`
    /// link, so only real disks are counted once.
    fn parse(text: &str, whole_disk: impl Fn(&str) -> bool) -> Vec<Self> {
        let mut disks = Vec::new();
        for line in text.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 13 || !whole_disk(fields[2]) {
                continue;
            }
            let number = |ix: usize| fields[ix].parse::<u64>().unwrap_or(0);
            disks.push(Self {
                name: fields[2].to_string(),
                read: number(5) * SECTOR,
                written: number(9) * SECTOR,
                busy_ms: number(12),
                requests: number(3) + number(7),
                request_ms: number(6) + number(10),
            });
        }
        disks
    }
}

/// Per-second rates between two network readings.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NetRates {
    pub received: f64,
    pub sent: f64,
}

impl NetRates {
    pub fn between(earlier: &NetCounters, later: &NetCounters, elapsed: Duration) -> Self {
        let seconds = elapsed.as_secs_f64();
        if seconds <= 0. {
            return Self::default();
        }
        Self {
            received: later.received.saturating_sub(earlier.received) as f64 / seconds,
            sent: later.sent.saturating_sub(earlier.sent) as f64 / seconds,
        }
    }
}

/// Per-second rates between two disk readings, plus how busy the disk was.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DiskRates {
    pub read: f64,
    pub written: f64,
    /// From 0 to 1.
    pub busy: f32,
    /// Average milliseconds per request, when any finished.
    pub response_ms: Option<f32>,
}

impl DiskRates {
    pub fn between(earlier: &DiskCounters, later: &DiskCounters, elapsed: Duration) -> Self {
        let seconds = elapsed.as_secs_f64();
        if seconds <= 0. {
            return Self::default();
        }
        let busy_ms = later.busy_ms.saturating_sub(earlier.busy_ms) as f64;
        let requests = later.requests.saturating_sub(earlier.requests);
        let request_ms = later.request_ms.saturating_sub(earlier.request_ms);
        Self {
            read: later.read.saturating_sub(earlier.read) as f64 / seconds,
            written: later.written.saturating_sub(earlier.written) as f64 / seconds,
            busy: (busy_ms / (seconds * 1000.)).clamp(0., 1.) as f32,
            response_ms: (requests > 0).then(|| request_ms as f32 / requests as f32),
        }
    }

    /// The whole machine: rates add up, and busy is the busiest single disk,
    /// since disks work in parallel.
    pub fn combined(disks: impl IntoIterator<Item = DiskRates>) -> Self {
        disks.into_iter().fold(Self::default(), |total, disk| Self {
            read: total.read + disk.read,
            written: total.written + disk.written,
            busy: total.busy.max(disk.busy),
            response_ms: None,
        })
    }
}

/// One disk as a snapshot shows it.
#[derive(Clone, Debug, Default)]
pub struct Disk {
    pub name: String,
    pub info: DiskInfo,
    pub rates: DiskRates,
    pub volumes: Vec<Volume>,
}

/// One network interface as a snapshot shows it.
#[derive(Clone, Debug, Default)]
pub struct Interface {
    pub name: String,
    pub info: InterfaceInfo,
    pub rates: NetRates,
    /// Bytes since boot.
    pub total_received: u64,
    pub total_sent: u64,
}

/// Rates for each device in `later`, matched by name against `earlier`. A
/// device that just showed up reads as idle until its second reading.
pub fn rates_by_name<C, R: Default>(
    earlier: &[C],
    later: &[C],
    name: impl Fn(&C) -> &str,
    between: impl Fn(&C, &C) -> R,
) -> Vec<R> {
    later
        .iter()
        .map(|now| earlier.iter().find(|before| name(before) == name(now)).map_or_else(R::default, |before| between(before, now)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_physical_interfaces() {
        let text = "Inter-|   Receive  |  Transmit\n \
                    face |bytes    packets|bytes    packets\n    \
                    lo: 1000 10 0 0 0 0 0 0 1000 10 0 0 0 0 0 0\n \
                    wlan0: 4000 52 0 0 0 0 0 0 1200 25 0 0 0 0 0 0\n\
                    enp109s0: 50 1 0 0 0 0 0 0 30 1 0 0 0 0 0 0\n\
                    docker0: 900 9 0 0 0 0 0 0 900 9 0 0 0 0 0 0\n";
        let interfaces = NetCounters::parse(text, |name| name == "wlan0" || name == "enp109s0");
        assert_eq!(
            interfaces,
            [
                NetCounters { name: "wlan0".into(), received: 4000, sent: 1200 },
                NetCounters { name: "enp109s0".into(), received: 50, sent: 30 },
            ]
        );
    }

    #[test]
    fn reads_whole_disks_in_bytes() {
        let text = " 259       0 nvme1n1 106 0 6508 52 0 0 0 0 0 41 52 0 0\n \
                    259       1 nvme0n1 69 39 3000 25 11 20 1000 16 1 3000 19 0 0\n \
                    259       2 nvme0n1p1 9 0 100 1 0 0 0 0 0 1 1 0 0\n \
                    252       0 zram0 99 0 9999 0 99 0 9999 0 0 0 0 0 0\n";
        let disks = DiskCounters::parse(text, |name| name.starts_with("nvme") && !name.contains('p'));
        assert_eq!(disks.len(), 2);
        assert_eq!((disks[0].name.as_str(), disks[0].read, disks[0].requests, disks[0].request_ms), ("nvme1n1", 6508 * 512, 106, 52));
        assert_eq!((disks[1].read, disks[1].written, disks[1].busy_ms), (3000 * 512, 1000 * 512, 3000));
        assert_eq!((disks[1].requests, disks[1].request_ms), (80, 41));
    }

    #[test]
    fn rates_divide_by_elapsed_time() {
        let half = Duration::from_millis(500);
        let net = |received, sent| NetCounters { name: "wlan0".into(), received, sent };
        assert_eq!(NetRates::between(&net(0, 100), &net(1000, 50), half), NetRates { received: 2000., sent: 0. });

        let disk = |read, written, busy_ms, requests, request_ms| DiskCounters { name: "sda".into(), read, written, busy_ms, requests, request_ms };
        let rates = DiskRates::between(&disk(0, 0, 1000, 10, 50), &disk(512, 1024, 1250, 14, 70), half);
        assert_eq!((rates.read, rates.written, rates.busy, rates.response_ms), (1024., 2048., 0.5, Some(5.)));
        let idle = DiskRates::between(&disk(0, 0, 0, 3, 9), &disk(0, 0, 0, 3, 9), half);
        assert_eq!(idle.response_ms, None);
        assert_eq!(DiskRates::between(&disk(0, 0, 0, 0, 0), &disk(1, 1, 1, 1, 1), Duration::ZERO), DiskRates::default());
    }

    #[test]
    fn combines_disks_and_matches_devices_by_name() {
        let a = DiskRates { read: 1., written: 2., busy: 0.25, response_ms: Some(1.) };
        let b = DiskRates { read: 3., written: 4., busy: 0.75, response_ms: Some(9.) };
        assert_eq!(DiskRates::combined([a, b]), DiskRates { read: 4., written: 6., busy: 0.75, response_ms: None });

        let earlier = [("a", 10), ("b", 20)];
        let later = [("b", 25), ("c", 5), ("a", 11)];
        let rates = rates_by_name(&earlier, &later, |device| device.0, |before, now| now.1 - before.1);
        assert_eq!(rates, [5, 0, 1]);
    }
}
