//! CPU clock speed and temperature from sysfs.

use std::path::{Path, PathBuf};

/// Average current clock across logical CPUs, and the fastest any can go.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Clock {
    pub current_mhz: f32,
    pub max_mhz: f32,
}

impl Clock {
    /// Current speed as a share of the maximum, from 0 to 1.
    pub fn fraction(&self) -> f32 {
        if self.max_mhz <= 0. { 0. } else { (self.current_mhz / self.max_mhz).clamp(0., 1.) }
    }
}

pub fn read_clock() -> Option<Clock> {
    let mut current = Vec::new();
    let mut max_khz = 0u64;
    for entry in std::fs::read_dir("/sys/devices/system/cpu").ok()?.filter_map(Result::ok) {
        let name = entry.file_name();
        let Some(index) = name.to_str().and_then(|name| name.strip_prefix("cpu")) else {
            continue;
        };
        if index.is_empty() || !index.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let freq = entry.path().join("cpufreq");
        if let Some(khz) = read_u64(&freq.join("scaling_cur_freq")) {
            current.push(khz);
        }
        max_khz = max_khz.max(read_u64(&freq.join("cpuinfo_max_freq")).unwrap_or(0));
    }
    if current.is_empty() {
        return None;
    }
    let average_khz = current.iter().sum::<u64>() as f32 / current.len() as f32;
    Some(Clock { current_mhz: average_khz / 1000., max_mhz: max_khz as f32 / 1000. })
}

/// hwmon drivers that report the CPU package temperature.
const CPU_HWMON: [&str; 3] = ["coretemp", "k10temp", "zenpower"];
/// Sensor labels for the whole package, best first.
const PACKAGE_LABELS: [&str; 3] = ["Package id 0", "Tctl", "Tdie"];

/// Finds the file with the CPU package temperature, once at startup.
pub fn find_cpu_temperature() -> Option<PathBuf> {
    for entry in std::fs::read_dir("/sys/class/hwmon").ok()?.filter_map(Result::ok) {
        let dir = entry.path();
        let name = std::fs::read_to_string(dir.join("name")).unwrap_or_default();
        if !CPU_HWMON.contains(&name.trim()) {
            continue;
        }
        let labels = temperature_labels(&dir);
        if let Some(input) = pick_package_sensor(&labels) {
            return Some(dir.join(input));
        }
    }
    // Fall back to the thermal zone the kernel ties to the CPU package.
    for entry in std::fs::read_dir("/sys/class/thermal").ok()?.filter_map(Result::ok) {
        let dir = entry.path();
        if std::fs::read_to_string(dir.join("type")).is_ok_and(|kind| kind.trim() == "x86_pkg_temp") {
            return Some(dir.join("temp"));
        }
    }
    None
}

/// `(tempN_input file name, label)` for every temperature sensor in a hwmon folder.
fn temperature_labels(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut sensors: Vec<(String, String)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name().to_str()?.to_string();
            let stem = file.strip_prefix("temp")?.strip_suffix("_input")?;
            let label = std::fs::read_to_string(dir.join(format!("temp{stem}_label"))).unwrap_or_default();
            Some((file, label.trim().to_string()))
        })
        .collect();
    sensors.sort();
    sensors
}

/// The package sensor when one is labeled, otherwise the first sensor.
fn pick_package_sensor(sensors: &[(String, String)]) -> Option<&str> {
    PACKAGE_LABELS
        .iter()
        .find_map(|wanted| sensors.iter().find(|(_, label)| label == wanted))
        .or_else(|| sensors.first())
        .map(|(file, _)| file.as_str())
}

/// Degrees Celsius from a millidegree file.
pub fn read_celsius(path: &Path) -> Option<f32> {
    read_u64(path).map(|milli| milli as f32 / 1000.)
}

pub fn read_u64(path: &Path) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensors(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(file, label)| (file.to_string(), label.to_string())).collect()
    }

    #[test]
    fn prefers_the_package_sensor() {
        let intel = sensors(&[("temp1_input", "Core 0"), ("temp2_input", "Package id 0")]);
        assert_eq!(pick_package_sensor(&intel), Some("temp2_input"));
        let amd = sensors(&[("temp1_input", "Tccd1"), ("temp3_input", "Tctl")]);
        assert_eq!(pick_package_sensor(&amd), Some("temp3_input"));
        let unlabeled = sensors(&[("temp1_input", "")]);
        assert_eq!(pick_package_sensor(&unlabeled), Some("temp1_input"));
        assert_eq!(pick_package_sensor(&[]), None);
    }

    #[test]
    fn clock_fraction_stays_in_range() {
        assert_eq!(Clock { current_mhz: 2800., max_mhz: 5600. }.fraction(), 0.5);
        assert_eq!(Clock { current_mhz: 6000., max_mhz: 5600. }.fraction(), 1.);
        assert_eq!(Clock::default().fraction(), 0.);
    }
}
