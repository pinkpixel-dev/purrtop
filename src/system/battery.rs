//! Laptop batteries and the AC adapter from `/sys/class/power_supply`.
//!
//! Batteries report in one of two styles: `energy_*` in µWh with `power_now`
//! in µW, or `charge_*` in µAh with `current_now` in µA and `voltage_now` in µV.

use std::path::Path;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BatteryState {
    Charging,
    Discharging,
    Full,
    NotCharging,
    #[default]
    Unknown,
}

impl BatteryState {
    fn parse(text: &str) -> Self {
        match text {
            "Charging" => BatteryState::Charging,
            "Discharging" => BatteryState::Discharging,
            "Full" => BatteryState::Full,
            "Not charging" => BatteryState::NotCharging,
            _ => BatteryState::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BatteryState::Charging => "Charging",
            BatteryState::Discharging => "On battery",
            BatteryState::Full => "Full",
            BatteryState::NotCharging => "Not charging",
            BatteryState::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Battery {
    /// The kernel name, like `BAT1`.
    pub name: String,
    pub state: BatteryState,
    /// From 0 to 1.
    pub charge: f32,
    /// Power going in or out, in watts.
    pub watts: Option<f32>,
    /// Until empty while discharging, until full while charging.
    pub time_left: Option<Duration>,
    /// Full capacity now over full capacity when new, from 0 to 1.
    pub health: Option<f32>,
    pub cycles: Option<u32>,
    pub full_wh: Option<f32>,
    pub design_wh: Option<f32>,
    pub model: Option<String>,
    pub manufacturer: Option<String>,
    pub technology: Option<String>,
}

/// Every laptop battery, plus whether the machine is plugged in.
pub fn read() -> (Vec<Battery>, bool) {
    let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else {
        return (Vec::new(), false);
    };
    let mut batteries = Vec::new();
    let mut plugged_in = false;
    for entry in entries.filter_map(Result::ok) {
        let dir = entry.path();
        let field = |key: &str| read_field(&dir, key);
        match field("type").as_deref() {
            Some("Mains") => plugged_in |= field("online").as_deref() == Some("1"),
            // Mice and headsets report `scope` Device; the laptop battery doesn't.
            Some("Battery") if field("scope").as_deref() != Some("Device") && field("present").as_deref() != Some("0") => {
                let name = entry.file_name().to_string_lossy().into_owned();
                batteries.push(Battery::parse(name, field));
            }
            _ => {}
        }
    }
    batteries.sort_by(|a, b| a.name.cmp(&b.name));
    (batteries, plugged_in)
}

fn read_field(dir: &Path, key: &str) -> Option<String> {
    let text = std::fs::read_to_string(dir.join(key)).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

impl Battery {
    fn parse(name: String, field: impl Fn(&str) -> Option<String>) -> Self {
        let number = |key: &str| field(key).and_then(|value| value.parse::<f64>().ok());
        let state = field("status").map_or(BatteryState::Unknown, |status| BatteryState::parse(&status));
        let voltage = number("voltage_now").map(|microvolts| microvolts / 1e6);
        let design_voltage = number("voltage_min_design").map(|microvolts| microvolts / 1e6).or(voltage);

        // Everything in watt-hours and watts, whichever style the battery uses.
        let (now_wh, full_wh, design_wh, watts) = if number("energy_full").is_some() {
            let wh = |key: &str| number(key).map(|microwatt_hours| microwatt_hours / 1e6);
            (wh("energy_now"), wh("energy_full"), wh("energy_full_design"), number("power_now").map(|microwatts| microwatts / 1e6))
        } else {
            let ah = |key: &str| number(key).map(|microamp_hours| microamp_hours / 1e6);
            let watts = number("current_now").zip(voltage).map(|(microamps, volts)| microamps.abs() / 1e6 * volts);
            (
                ah("charge_now").zip(voltage).map(|(ah, volts)| ah * volts),
                ah("charge_full").zip(design_voltage).map(|(ah, volts)| ah * volts),
                ah("charge_full_design").zip(design_voltage).map(|(ah, volts)| ah * volts),
                watts,
            )
        };

        let charge = number("capacity").map(|percent| percent / 100.).or_else(|| now_wh.zip(full_wh).map(|(now, full)| now / full));
        let time_left = match (state, now_wh, full_wh, watts) {
            (BatteryState::Discharging, Some(now), _, Some(watts)) if watts > 0. => Some(now / watts),
            (BatteryState::Charging, Some(now), Some(full), Some(watts)) if watts > 0. => Some((full - now).max(0.) / watts),
            _ => None,
        };

        Self {
            name,
            state,
            charge: charge.unwrap_or(0.).clamp(0., 1.) as f32,
            watts: watts.map(|watts| watts as f32),
            time_left: time_left.map(|hours| Duration::from_secs_f64(hours * 3600.)),
            health: full_wh.zip(design_wh).filter(|&(_, design)| design > 0.).map(|(full, design)| (full / design) as f32),
            cycles: field("cycle_count").and_then(|count| count.parse().ok()).filter(|&count| count > 0),
            full_wh: full_wh.map(|wh| wh as f32),
            design_wh: design_wh.map(|wh| wh as f32),
            model: field("model_name"),
            manufacturer: field("manufacturer"),
            technology: field("technology"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn parse(fields: &[(&str, &str)]) -> Battery {
        let fields: HashMap<&str, &str> = fields.iter().copied().collect();
        Battery::parse("BAT1".into(), |key| fields.get(key).map(|value| value.to_string()))
    }

    #[test]
    fn reads_charge_style_batteries() {
        let battery = parse(&[
            ("status", "Discharging"),
            ("capacity", "50"),
            ("charge_now", "2500000"),
            ("charge_full", "5000000"),
            ("charge_full_design", "6250000"),
            ("current_now", "1000000"),
            ("voltage_now", "16000000"),
            ("voltage_min_design", "16000000"),
            ("cycle_count", "199"),
            ("model_name", "AP21A8T"),
        ]);
        assert_eq!(battery.state, BatteryState::Discharging);
        assert_eq!(battery.charge, 0.5);
        assert_eq!(battery.watts, Some(16.));
        // 2.5 Ah × 16 V = 40 Wh, at 16 W: two and a half hours.
        assert_eq!(battery.time_left, Some(Duration::from_secs(9000)));
        assert_eq!(battery.health, Some(0.8));
        assert_eq!((battery.full_wh, battery.design_wh), (Some(80.), Some(100.)));
        assert_eq!((battery.cycles, battery.model.as_deref()), (Some(199), Some("AP21A8T")));
    }

    #[test]
    fn reads_energy_style_batteries_while_charging() {
        let battery = parse(&[
            ("status", "Charging"),
            ("energy_now", "30000000"),
            ("energy_full", "60000000"),
            ("energy_full_design", "60000000"),
            ("power_now", "15000000"),
        ]);
        assert_eq!(battery.charge, 0.5);
        assert_eq!(battery.time_left, Some(Duration::from_secs(7200)));
        assert_eq!(battery.health, Some(1.));
        assert_eq!(battery.cycles, None);
    }

    #[test]
    fn full_or_idle_batteries_have_no_time_left() {
        let full = parse(&[("status", "Full"), ("capacity", "100"), ("charge_now", "5"), ("current_now", "0"), ("voltage_now", "17000000")]);
        assert_eq!((full.state, full.watts, full.time_left), (BatteryState::Full, Some(0.), None));
        assert_eq!(parse(&[("status", "Weird")]).state.label(), "Unknown");
    }
}
