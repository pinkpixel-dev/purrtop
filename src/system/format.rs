//! How numbers read on screen.

use std::time::Duration;

/// Binary sizes with one decimal: `8.3 GB`, `512.0 MB`, `0 B`.
pub fn bytes(value: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    if value < 1024 {
        return format!("{value} B");
    }
    let mut size = value as f64;
    let mut unit = 0;
    while size >= 1024. && unit < UNITS.len() - 1 {
        size /= 1024.;
        unit += 1;
    }
    format!("{size:.1} {}", UNITS[unit])
}

/// Bytes per second: `1.2 MB/s`.
pub fn rate(bytes_per_second: f64) -> String {
    format!("{}/s", bytes(bytes_per_second.max(0.) as u64))
}

/// `1.41 GHz`, or `800 MHz` under a gigahertz.
pub fn frequency(mhz: f32) -> String {
    if mhz >= 1000. { format!("{:.2} GHz", mhz / 1000.) } else { format!("{mhz:.0} MHz") }
}

/// Whole degrees: `62 °C`.
pub fn celsius(degrees: f32) -> String {
    format!("{degrees:.0} °C")
}

/// A 0 to 1 share as a percentage with one decimal.
pub fn percent(fraction: f32) -> String {
    format!("{:.1}%", fraction * 100.)
}

/// Link speed: `1 Gbit/s`, `2.5 Gbit/s`, `100 Mbit/s`.
pub fn link_speed(mbps: u32) -> String {
    if mbps >= 1000 {
        let gbps = format!("{:.1}", mbps as f32 / 1000.);
        format!("{} Gbit/s", gbps.trim_end_matches(".0"))
    } else {
        format!("{mbps} Mbit/s")
    }
}

/// `0.4 ms`, or whole milliseconds from 10 up.
pub fn milliseconds(ms: f32) -> String {
    if ms < 10. { format!("{ms:.1} ms") } else { format!("{ms:.0} ms") }
}

/// Rough time left: `2 h 15 min`, or `45 min` under an hour.
pub fn duration(duration: Duration) -> String {
    let minutes = duration.as_secs() / 60;
    if minutes >= 60 { format!("{} h {} min", minutes / 60, minutes % 60) } else { format!("{minutes} min") }
}

/// How long ago something happened: `just now`, `12 min ago`, `3 h ago`, `2 days ago`.
pub fn ago(elapsed: Duration) -> String {
    let minutes = elapsed.as_secs() / 60;
    match minutes {
        0 => "just now".into(),
        1..60 => format!("{minutes} min ago"),
        60..1440 => format!("{} h ago", minutes / 60),
        1440..2880 => "1 day ago".into(),
        _ => format!("{} days ago", minutes / 1440),
    }
}

/// `3d 4:05:06`, or `4:05:06` under a day.
pub fn uptime(duration: Duration) -> String {
    let total = duration.as_secs();
    let (days, hours, minutes, seconds) = (total / 86_400, total / 3600 % 24, total / 60 % 60, total % 60);
    if days > 0 {
        format!("{days}d {hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{hours}:{minutes:02}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_sizes() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1536), "1.5 KB");
        assert_eq!(bytes(8_912_896_000), "8.3 GB");
    }

    #[test]
    fn formats_rates_clocks_and_temperatures() {
        assert_eq!(rate(1536.), "1.5 KB/s");
        assert_eq!(rate(-5.), "0 B/s");
        assert_eq!(frequency(1410.), "1.41 GHz");
        assert_eq!(frequency(800.4), "800 MHz");
        assert_eq!(celsius(61.6), "62 °C");
        assert_eq!(link_speed(1000), "1 Gbit/s");
        assert_eq!(link_speed(2500), "2.5 Gbit/s");
        assert_eq!(link_speed(100), "100 Mbit/s");
        assert_eq!(milliseconds(0.42), "0.4 ms");
        assert_eq!(milliseconds(12.6), "13 ms");
    }

    #[test]
    fn formats_time_ago() {
        assert_eq!(ago(Duration::from_secs(30)), "just now");
        assert_eq!(ago(Duration::from_secs(12 * 60)), "12 min ago");
        assert_eq!(ago(Duration::from_secs(3 * 3600 + 59)), "3 h ago");
        assert_eq!(ago(Duration::from_secs(30 * 3600)), "1 day ago");
        assert_eq!(ago(Duration::from_secs(5 * 86_400)), "5 days ago");
    }

    #[test]
    fn formats_percent_and_uptime() {
        assert_eq!(percent(0.143), "14.3%");
        assert_eq!(uptime(Duration::from_secs(16 * 60 + 56)), "0:16:56");
        assert_eq!(duration(Duration::from_secs(9000)), "2 h 30 min");
        assert_eq!(duration(Duration::from_secs(45 * 60 + 30)), "45 min");
        assert_eq!(uptime(Duration::from_secs(3 * 86_400 + 4 * 3600 + 5 * 60 + 6)), "3d 4:05:06");
    }
}
