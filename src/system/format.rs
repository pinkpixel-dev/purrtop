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

/// A 0 to 1 share as a percentage with one decimal.
pub fn percent(fraction: f32) -> String {
    format!("{:.1}%", fraction * 100.)
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
    fn formats_percent_and_uptime() {
        assert_eq!(percent(0.143), "14.3%");
        assert_eq!(uptime(Duration::from_secs(16 * 60 + 56)), "0:16:56");
        assert_eq!(uptime(Duration::from_secs(3 * 86_400 + 4 * 3600 + 5 * 60 + 6)), "3d 4:05:06");
    }
}
