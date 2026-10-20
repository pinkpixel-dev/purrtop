//! Temperature sensors from `/sys/class/hwmon`, found once at startup and
//! given names people recognize.
//!
//! GPU sensors are left out on purpose: reading a sleeping GPU's hwmon can
//! wake it, so GPU temperatures only come from [`super::gpu`], which checks first.

use std::path::{Path, PathBuf};

use super::sensors::read_celsius;

/// A sensor, or a group of sensors read as their hottest (every CPU core).
#[derive(Clone, Debug)]
pub struct ThermalSource {
    /// Stable across readings, for histories: `hwmon8/cores`.
    pub key: String,
    pub label: String,
    inputs: Vec<PathBuf>,
    critical: Option<f32>,
}

/// One reading of a [`ThermalSource`].
#[derive(Clone, Debug, PartialEq)]
pub struct Temperature {
    pub key: String,
    pub label: String,
    pub celsius: f32,
    /// Where the hardware says it's too hot, when it says.
    pub critical: Option<f32>,
}

impl ThermalSource {
    /// Every temperature sensor worth showing, CPU first.
    pub fn discover() -> Vec<Self> {
        let Ok(entries) = std::fs::read_dir("/sys/class/hwmon") else {
            return Vec::new();
        };
        let mut chips: Vec<PathBuf> = entries.filter_map(Result::ok).map(|entry| entry.path()).collect();
        // hwmon10 after hwmon9.
        chips.sort_by_key(|path| path.file_name().and_then(|name| name.to_str()).and_then(|name| name.strip_prefix("hwmon")?.parse::<u32>().ok()));

        let mut sources = Vec::new();
        let mut memory_modules = 0;
        for chip in chips {
            if chip.join("device/drm").exists() {
                continue;
            }
            let name = std::fs::read_to_string(chip.join("name")).unwrap_or_default().trim().to_string();
            let sensors = sensors(&chip);
            if sensors.is_empty() {
                continue;
            }
            let id = chip.file_name().and_then(|name| name.to_str()).unwrap_or_default().to_string();
            let network = network_name(&chip).or_else(|| interface_for_driver(&name));
            let context = Context { memory_modules: &mut memory_modules, network, nvme: nvme_disk(&chip) };
            sources.extend(group(&id, &name, sensors, context));
        }
        sources.sort_by_key(|source| !source.key.ends_with("/package") && !source.key.ends_with("/cores"));
        sources
    }

    pub fn read(&self) -> Option<Temperature> {
        let celsius = self.inputs.iter().filter_map(|path| read_celsius(path)).reduce(f32::max)?;
        Some(Temperature { key: self.key.clone(), label: self.label.clone(), celsius, critical: self.critical })
    }
}

/// One `tempN_input` in a chip folder.
#[derive(Clone, Debug, PartialEq)]
struct Sensor {
    input: PathBuf,
    label: String,
    critical: Option<f32>,
}

fn sensors(chip: &Path) -> Vec<Sensor> {
    let Ok(entries) = std::fs::read_dir(chip) else {
        return Vec::new();
    };
    let mut sensors: Vec<(u32, Sensor)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name().to_str()?.to_string();
            let index: u32 = file.strip_prefix("temp")?.strip_suffix("_input")?.parse().ok()?;
            let label = std::fs::read_to_string(chip.join(format!("temp{index}_label"))).unwrap_or_default().trim().to_string();
            let critical = read_celsius(&chip.join(format!("temp{index}_crit"))).filter(|&celsius| celsius > 0.);
            Some((index, Sensor { input: entry.path(), label, critical }))
        })
        .collect();
    sensors.sort_by_key(|(index, _)| *index);
    sensors.into_iter().map(|(_, sensor)| sensor).collect()
}

/// What a chip's device tells us about it, for naming.
struct Context<'a> {
    memory_modules: &'a mut usize,
    /// The interface name when the chip belongs to a network card.
    network: Option<String>,
    /// The namespace name (`nvme0n1`) when the chip is an NVMe controller.
    nvme: Option<String>,
}

/// Turns one chip's sensors into the sources shown in the list.
fn group(id: &str, chip: &str, sensors: Vec<Sensor>, context: Context) -> Vec<ThermalSource> {
    let source = |key: &str, label: String, sensors: &[&Sensor]| ThermalSource {
        key: format!("{id}/{key}"),
        label,
        inputs: sensors.iter().map(|sensor| sensor.input.clone()).collect(),
        critical: sensors.iter().filter_map(|sensor| sensor.critical).reduce(f32::min),
    };
    let all: Vec<&Sensor> = sensors.iter().collect();
    match chip {
        "coretemp" | "k10temp" | "zenpower" => {
            let is_package = |sensor: &&Sensor| matches!(sensor.label.as_str(), "Package id 0" | "Tctl" | "Tdie");
            let package: Vec<&Sensor> = sensors.iter().filter(is_package).take(1).collect();
            let cores: Vec<&Sensor> = sensors.iter().filter(|sensor| !is_package(sensor)).collect();
            let mut sources = Vec::new();
            if !package.is_empty() {
                sources.push(source("package", "CPU package".into(), &package));
            }
            if !cores.is_empty() {
                sources.push(source("cores", "CPU cores (hottest)".into(), &cores));
            }
            sources
        }
        "nvme" => {
            // The composite reading is the drive's own summary of its sensors.
            let composite: Vec<&Sensor> = sensors.iter().filter(|sensor| sensor.label == "Composite").take(1).collect();
            let picked = if composite.is_empty() { &all[..1] } else { &composite[..] };
            let label = context.nvme.map_or_else(|| "NVMe drive".to_string(), |disk| format!("NVMe {disk}"));
            vec![source("drive", label, picked)]
        }
        "spd5118" | "jc42" => {
            *context.memory_modules += 1;
            vec![source("module", format!("Memory module {}", context.memory_modules), &all)]
        }
        "acpitz" => vec![source("zone", "ACPI thermal zone".into(), &all)],
        _ if chip.starts_with("iwlwifi") => vec![source("wifi", "Wi-Fi card".into(), &all)],
        _ => {
            let name = context.network.map_or_else(|| chip.to_string(), |interface| format!("Network card ({interface})"));
            if all.len() == 1 {
                vec![source("temp", name, &all)]
            } else {
                // Several sensors on an unknown chip: name each by its label.
                sensors
                    .iter()
                    .enumerate()
                    .map(|(ix, sensor)| {
                        let label = if sensor.label.is_empty() { format!("{name} {}", ix + 1) } else { format!("{name} {}", sensor.label) };
                        source(&format!("temp{ix}"), label, &[sensor])
                    })
                    .collect()
            }
        }
    }
}

/// The interface name under `device/net`, for network card sensors.
fn network_name(chip: &Path) -> Option<String> {
    let mut entries = std::fs::read_dir(chip.join("device/net")).ok()?;
    entries.find_map(Result::ok)?.file_name().to_str().map(str::to_string)
}

/// Some network sensors hang off the PHY instead of the card, named after
/// the driver (`r8169_0_6d00:00`). Finds the interface that driver runs.
fn interface_for_driver(chip: &str) -> Option<String> {
    let driver = chip.split('_').next()?;
    std::fs::read_dir("/sys/class/net").ok()?.filter_map(Result::ok).find_map(|entry| {
        let link = std::fs::read_link(entry.path().join("device/driver")).ok()?;
        (link.file_name()?.to_str()? == driver).then(|| entry.file_name().to_str().map(str::to_string)).flatten()
    })
}

/// `nvme0n1` for the controller `nvme0`: the first namespace folder inside it.
fn nvme_disk(chip: &Path) -> Option<String> {
    let device = std::fs::canonicalize(chip.join("device")).ok()?;
    let controller = device.file_name()?.to_str()?.to_string();
    let prefix = format!("{controller}n");
    let mut namespaces: Vec<String> = std::fs::read_dir(&device)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name.starts_with(&prefix) && !name.contains('p'))
        .collect();
    namespaces.sort();
    namespaces.into_iter().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sensor(index: u32, label: &str) -> Sensor {
        Sensor { input: PathBuf::from(format!("temp{index}_input")), label: label.into(), critical: None }
    }

    fn context(modules: &mut usize) -> Context<'_> {
        Context { memory_modules: modules, network: None, nvme: None }
    }

    #[test]
    fn splits_the_cpu_into_package_and_cores() {
        let mut modules = 0;
        let mut sensors = vec![sensor(1, "Package id 0"), sensor(2, "Core 0"), sensor(6, "Core 4")];
        sensors[1].critical = Some(100.);
        let sources = group("hwmon8", "coretemp", sensors, context(&mut modules));
        assert_eq!(sources.len(), 2);
        assert_eq!((sources[0].key.as_str(), sources[0].label.as_str(), sources[0].inputs.len()), ("hwmon8/package", "CPU package", 1));
        assert_eq!((sources[1].label.as_str(), sources[1].inputs.len(), sources[1].critical), ("CPU cores (hottest)", 2, Some(100.)));
    }

    #[test]
    fn names_drives_memory_and_network_cards() {
        let mut modules = 0;
        let nvme = Context { nvme: Some("nvme0n1".into()), ..context(&mut modules) };
        let drive = group("hwmon3", "nvme", vec![sensor(1, "Composite"), sensor(2, "Sensor 1")], nvme);
        assert_eq!((drive.len(), drive[0].label.as_str(), drive[0].inputs[0].to_str()), (1, "NVMe nvme0n1", Some("temp1_input")));

        let first = group("hwmon5", "spd5118", vec![sensor(1, "")], context(&mut modules));
        let second = group("hwmon6", "spd5118", vec![sensor(1, "")], context(&mut modules));
        assert_eq!((first[0].label.as_str(), second[0].label.as_str()), ("Memory module 1", "Memory module 2"));

        let card = Context { network: Some("enp109s0".into()), ..context(&mut modules) };
        let ethernet = group("hwmon7", "r8169_0_6d00:00", vec![sensor(1, "")], card);
        assert_eq!(ethernet[0].label, "Network card (enp109s0)");
        assert_eq!(group("hwmon9", "iwlwifi_1", vec![sensor(1, "")], context(&mut modules))[0].label, "Wi-Fi card");
    }

    #[test]
    fn unknown_chips_with_several_sensors_use_their_labels() {
        let mut modules = 0;
        let sources = group("hwmon2", "it8688", vec![sensor(1, "System"), sensor(2, "")], context(&mut modules));
        let labels: Vec<&str> = sources.iter().map(|source| source.label.as_str()).collect();
        assert_eq!(labels, ["it8688 System", "it8688 2"]);
    }

    #[test]
    fn reads_the_hottest_input() {
        let dir = std::env::temp_dir().join(format!("purrtop-thermals-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a"), "41000\n").unwrap();
        std::fs::write(dir.join("b"), "57500\n").unwrap();
        let source = ThermalSource { key: "k".into(), label: "l".into(), inputs: vec![dir.join("a"), dir.join("b"), dir.join("gone")], critical: None };
        assert_eq!(source.read().map(|reading| reading.celsius), Some(57.5));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
