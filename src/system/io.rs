//! Network and disk counters, turned into per-second rates between readings.

use std::path::Path;
use std::time::Duration;

/// Bytes received and sent across physical network interfaces.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetCounters {
    pub received: u64,
    pub sent: u64,
}

impl NetCounters {
    pub fn read() -> Option<Self> {
        let text = std::fs::read_to_string("/proc/net/dev").ok()?;
        Some(Self::parse(&text, |name| Path::new("/sys/class/net").join(name).join("device").exists()))
    }

    /// Sums the interfaces `physical` accepts. Loopback, bridges, Docker and
    /// VPN tunnels have no `device` link in sysfs, so they're left out.
    fn parse(text: &str, physical: impl Fn(&str) -> bool) -> Self {
        let mut counters = Self::default();
        for line in text.lines().skip(2) {
            let Some((name, fields)) = line.split_once(':') else {
                continue;
            };
            if !physical(name.trim()) {
                continue;
            }
            let fields: Vec<u64> = fields.split_whitespace().filter_map(|field| field.parse().ok()).collect();
            counters.received += fields.first().copied().unwrap_or(0);
            counters.sent += fields.get(8).copied().unwrap_or(0);
        }
        counters
    }
}

/// Bytes read and written on whole disks, plus milliseconds spent doing I/O.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiskCounters {
    pub read: u64,
    pub written: u64,
    /// The busiest disk's I/O time, since disks work in parallel.
    pub busy_ms: u64,
}

/// Sector size the kernel uses in `/proc/diskstats`, whatever the hardware's.
const SECTOR: u64 = 512;

impl DiskCounters {
    pub fn read() -> Option<Self> {
        let text = std::fs::read_to_string("/proc/diskstats").ok()?;
        Some(Self::parse(&text, |name| Path::new("/sys/block").join(name).join("device").exists()))
    }

    /// Counts the devices `whole_disk` accepts. Partitions aren't in
    /// `/sys/block`, and loop, zram and device-mapper entries have no `device`
    /// link, so only real disks are counted once.
    fn parse(text: &str, whole_disk: impl Fn(&str) -> bool) -> Self {
        let mut counters = Self::default();
        for line in text.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            if fields.len() < 13 || !whole_disk(fields[2]) {
                continue;
            }
            let number = |ix: usize| fields[ix].parse::<u64>().unwrap_or(0);
            counters.read += number(5) * SECTOR;
            counters.written += number(9) * SECTOR;
            counters.busy_ms = counters.busy_ms.max(number(12));
        }
        counters
    }
}

/// Per-second rates between two network readings.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NetRates {
    pub received: f64,
    pub sent: f64,
}

impl NetRates {
    pub fn between(earlier: NetCounters, later: NetCounters, elapsed: Duration) -> Self {
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

/// Per-second rates between two disk readings, plus how busy the busiest disk was.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DiskRates {
    pub read: f64,
    pub written: f64,
    /// From 0 to 1.
    pub busy: f32,
}

impl DiskRates {
    pub fn between(earlier: DiskCounters, later: DiskCounters, elapsed: Duration) -> Self {
        let seconds = elapsed.as_secs_f64();
        if seconds <= 0. {
            return Self::default();
        }
        let busy_ms = later.busy_ms.saturating_sub(earlier.busy_ms) as f64;
        Self {
            read: later.read.saturating_sub(earlier.read) as f64 / seconds,
            written: later.written.saturating_sub(earlier.written) as f64 / seconds,
            busy: (busy_ms / (seconds * 1000.)).clamp(0., 1.) as f32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_only_physical_interfaces() {
        let text = "Inter-|   Receive  |  Transmit\n \
                    face |bytes    packets|bytes    packets\n    \
                    lo: 1000 10 0 0 0 0 0 0 1000 10 0 0 0 0 0 0\n \
                    wlan0: 4000 52 0 0 0 0 0 0 1200 25 0 0 0 0 0 0\n\
                    enp109s0: 50 1 0 0 0 0 0 0 30 1 0 0 0 0 0 0\n\
                    docker0: 900 9 0 0 0 0 0 0 900 9 0 0 0 0 0 0\n";
        let counters = NetCounters::parse(text, |name| name == "wlan0" || name == "enp109s0");
        assert_eq!(counters, NetCounters { received: 4050, sent: 1230 });
    }

    #[test]
    fn counts_whole_disks_in_bytes() {
        let text = " 259       0 nvme1n1 106 0 6508 52 0 0 0 0 0 41 52 0 0\n \
                    259       1 nvme0n1 69 39 3000 25 11 20 1000 16 1 3000 19 0 0\n \
                    259       2 nvme0n1p1 9 0 100 1 0 0 0 0 0 1 1 0 0\n \
                    252       0 zram0 99 0 9999 0 99 0 9999 0 0 0 0 0 0\n";
        let counters = DiskCounters::parse(text, |name| name.starts_with("nvme") && !name.contains('p'));
        assert_eq!(counters.read, (6508 + 3000) * 512);
        assert_eq!(counters.written, 1000 * 512);
        assert_eq!(counters.busy_ms, 3000);
    }

    #[test]
    fn rates_divide_by_elapsed_time() {
        let second = Duration::from_millis(500);
        let net = NetRates::between(NetCounters { received: 0, sent: 100 }, NetCounters { received: 1000, sent: 50 }, second);
        assert_eq!(net, NetRates { received: 2000., sent: 0. });

        let earlier = DiskCounters { read: 0, written: 0, busy_ms: 1000 };
        let later = DiskCounters { read: 512, written: 1024, busy_ms: 1250 };
        let disk = DiskRates::between(earlier, later, second);
        assert_eq!((disk.read, disk.written, disk.busy), (1024., 2048., 0.5));
        assert_eq!(DiskRates::between(earlier, later, Duration::ZERO), DiskRates::default());
    }
}
