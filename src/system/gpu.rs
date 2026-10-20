//! GPUs from `/sys/class/drm`, read only from sysfs files that never wake a
//! sleeping GPU. NVIDIA's NVML would give more detail, but holding it open
//! keeps a laptop's dGPU out of runtime suspend.

use std::path::{Path, PathBuf};

use super::sensors::{read_celsius, read_u64};

/// A GPU found at startup: where to read it and what to call it.
#[derive(Clone, Debug)]
pub struct GpuDevice {
    pub name: String,
    pub vendor: Vendor,
    /// The kernel driver, like `i915`, `amdgpu` or `nvidia`.
    pub driver: Option<String>,
    /// The PCI address, like `0000:01:00.0`. Unique, so histories key on it.
    pub slot: String,
    /// The card folder, `/sys/class/drm/cardN`.
    card: PathBuf,
    /// The PCI device folder, `/sys/class/drm/cardN/device`.
    path: PathBuf,
    hwmon: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Vendor {
    Amd,
    Intel,
    Nvidia,
    Other,
}

impl Vendor {
    fn from_id(id: u32) -> Self {
        match id {
            0x1002 => Vendor::Amd,
            0x8086 => Vendor::Intel,
            0x10de => Vendor::Nvidia,
            _ => Vendor::Other,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Vendor::Amd => "AMD",
            Vendor::Intel => "Intel",
            Vendor::Nvidia => "NVIDIA",
            Vendor::Other => "Other",
        }
    }
}

/// One reading of a GPU. Fields the driver doesn't expose stay `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct GpuReading {
    pub name: String,
    pub vendor: Vendor,
    pub driver: Option<String>,
    pub slot: String,
    /// False while the GPU is runtime-suspended.
    pub awake: bool,
    /// The kernel's runtime PM status (`active`, `suspended`, `unsupported`).
    pub power_state: Option<String>,
    /// From 0 to 1.
    pub busy: Option<f32>,
    pub temperature: Option<f32>,
    pub watts: Option<f32>,
    /// Current and top clock. Read from i915's `gt_*_freq_mhz`.
    pub clock_mhz: Option<f32>,
    pub max_clock_mhz: Option<f32>,
    /// Video memory in bytes. Read from amdgpu's `mem_info_vram_*`.
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
}

impl GpuDevice {
    /// Every GPU, in card order. `pci_ids` is the text of `pci.ids`, used for names.
    pub fn discover(pci_ids: &str) -> Vec<Self> {
        let Ok(entries) = std::fs::read_dir("/sys/class/drm") else {
            return Vec::new();
        };
        let mut cards: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .and_then(|name| name.strip_prefix("card"))
                    .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
            })
            .collect();
        cards.sort();

        cards
            .into_iter()
            .filter_map(|card| {
                let path = card.join("device");
                let vendor_id = read_hex(&path.join("vendor"))?;
                let device_id = read_hex(&path.join("device"))?;
                let vendor = Vendor::from_id(vendor_id);
                let name = pci_name(pci_ids, vendor_id, device_id)
                    .unwrap_or_else(|| format!("GPU {vendor_id:04x}:{device_id:04x}"));
                let hwmon = std::fs::read_dir(path.join("hwmon"))
                    .ok()
                    .and_then(|mut entries| entries.find_map(Result::ok))
                    .map(|entry| entry.path());
                let link_name = |link: &Path| std::fs::read_link(link).ok()?.file_name()?.to_str().map(str::to_string);
                let driver = link_name(&path.join("driver"));
                let slot = std::fs::canonicalize(&path)
                    .ok()
                    .and_then(|real| real.file_name()?.to_str().map(str::to_string))
                    .unwrap_or_else(|| card.display().to_string());
                Some(Self { name, vendor, driver, slot, card, path, hwmon })
            })
            .collect()
    }

    pub fn read(&self) -> GpuReading {
        // Devices without runtime PM report nothing or "unsupported"; treat those as awake.
        let status = std::fs::read_to_string(self.path.join("power/runtime_status")).unwrap_or_default();
        let status = status.trim();
        let awake = status != "suspended";
        let hwmon = self.hwmon.as_deref().filter(|_| awake);
        let card_mhz = |file: &str| awake.then(|| read_u64(&self.card.join(file))).flatten().map(|mhz| mhz as f32);
        let device_bytes = |file: &str| awake.then(|| read_u64(&self.path.join(file))).flatten();
        GpuReading {
            name: self.name.clone(),
            vendor: self.vendor,
            driver: self.driver.clone(),
            slot: self.slot.clone(),
            awake,
            power_state: (!status.is_empty()).then(|| status.to_string()),
            clock_mhz: card_mhz("gt_act_freq_mhz"),
            max_clock_mhz: card_mhz("gt_RP0_freq_mhz"),
            vram_used: device_bytes("mem_info_vram_used"),
            vram_total: device_bytes("mem_info_vram_total"),
            busy: awake
                .then(|| read_u64(&self.path.join("gpu_busy_percent")))
                .flatten()
                .map(|percent| (percent as f32 / 100.).clamp(0., 1.)),
            temperature: hwmon.and_then(|dir| read_celsius(&dir.join("temp1_input"))),
            watts: hwmon.and_then(|dir| {
                read_u64(&dir.join("power1_average"))
                    .or_else(|| read_u64(&dir.join("power1_input")))
                    .map(|microwatts| microwatts as f32 / 1_000_000.)
            }),
        }
    }
}

fn read_hex(path: &Path) -> Option<u32> {
    let text = std::fs::read_to_string(path).ok()?;
    u32::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()
}

/// The device name from `pci.ids`. When the name has a marketing name in
/// brackets (`AD103M [GeForce RTX 4090 Laptop GPU]`), that's the one used.
pub fn pci_name(pci_ids: &str, vendor: u32, device: u32) -> Option<String> {
    let vendor_prefix = format!("{vendor:04x}  ");
    let device_prefix = format!("\t{device:04x}  ");
    let mut in_vendor = false;
    for line in pci_ids.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        if !line.starts_with('\t') {
            if in_vendor {
                break;
            }
            in_vendor = line.starts_with(&vendor_prefix);
            continue;
        }
        if in_vendor && let Some(name) = line.strip_prefix(&device_prefix) {
            let name = name.trim();
            let bracketed = name.rfind('[').and_then(|start| name[start + 1..].strip_suffix(']'));
            return Some(bracketed.unwrap_or(name).to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDS: &str = "# comment\n\
                       8086  Intel Corporation\n\
                       \ta788  Raptor Lake-S UHD Graphics\n\
                       \t\t1043 1e83  subsystem line\n\
                       10de  NVIDIA Corporation\n\
                       \t2757  AD103M / GN21-X11 [GeForce RTX 4090 Laptop GPU]\n\
                       10df  Emulex Corporation\n\
                       \t2757  Not this one\n";

    #[test]
    fn finds_names_and_prefers_the_bracketed_one() {
        assert_eq!(pci_name(IDS, 0x8086, 0xa788).as_deref(), Some("Raptor Lake-S UHD Graphics"));
        assert_eq!(pci_name(IDS, 0x10de, 0x2757).as_deref(), Some("GeForce RTX 4090 Laptop GPU"));
        assert_eq!(pci_name(IDS, 0x10de, 0x0001), None);
        assert_eq!(pci_name(IDS, 0x1002, 0x2757), None);
    }

    #[test]
    fn maps_vendor_ids() {
        assert_eq!(Vendor::from_id(0x10de), Vendor::Nvidia);
        assert_eq!(Vendor::from_id(0x1002), Vendor::Amd);
        assert_eq!(Vendor::from_id(0x1234), Vendor::Other);
    }
}
