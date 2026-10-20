//! What the machine is: maker, model, board and firmware from DMI, read once.
//! Serial numbers and the UUID are root-only, so they're left out.

use std::path::Path;

const DMI: &str = "/sys/class/dmi/id";
/// The Secure Boot variable: 4 bytes of attributes, then 1 when it's on.
const SECURE_BOOT: &str = "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
/// Values firmware vendors leave in when nobody filled the field in.
const PLACEHOLDERS: [&str; 12] = [
    "to be filled by o.e.m.",
    "default string",
    "system product name",
    "system manufacturer",
    "system version",
    "not applicable",
    "not specified",
    "none",
    "n/a",
    "oem",
    "0123456789",
    "type1productconfigid",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HardwareInfo {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub family: Option<String>,
    pub version: Option<String>,
    pub chassis: Option<&'static str>,
    pub board: Option<String>,
    pub firmware: Option<String>,
    pub firmware_date: Option<String>,
    pub uefi: bool,
    /// `None` on legacy BIOS boots, where there's no such thing.
    pub secure_boot: Option<bool>,
    /// The loaded NVIDIA kernel module's version. Reading it doesn't touch the GPU.
    pub nvidia_driver: Option<String>,
}

impl HardwareInfo {
    pub fn read() -> Self {
        let dmi = |file: &str| clean(&std::fs::read_to_string(Path::new(DMI).join(file)).unwrap_or_default());
        let board = join_names(dmi("board_vendor"), dmi("board_name"));
        let firmware = join_names(dmi("bios_vendor"), dmi("bios_version"));
        Self {
            manufacturer: dmi("sys_vendor"),
            model: dmi("product_name"),
            family: dmi("product_family"),
            version: dmi("product_version"),
            chassis: dmi("chassis_type").and_then(|code| code.parse().ok()).and_then(chassis_name),
            board,
            firmware,
            firmware_date: dmi("bios_date"),
            uefi: Path::new("/sys/firmware/efi").exists(),
            secure_boot: std::fs::read(SECURE_BOOT).ok().and_then(|bytes| secure_boot(&bytes)),
            nvidia_driver: clean(&std::fs::read_to_string("/sys/module/nvidia/version").unwrap_or_default()),
        }
    }
}

/// Trims a DMI value and drops it when it's empty or a vendor placeholder.
fn clean(value: &str) -> Option<String> {
    let value = value.trim();
    let placeholder = PLACEHOLDERS.iter().any(|placeholder| value.eq_ignore_ascii_case(placeholder));
    (!value.is_empty() && !placeholder).then(|| value.to_string())
}

/// `vendor name`, or whichever one is there.
fn join_names(vendor: Option<String>, name: Option<String>) -> Option<String> {
    match (vendor, name) {
        (Some(vendor), Some(name)) => Some(format!("{vendor} {name}")),
        (vendor, name) => vendor.or(name),
    }
}

/// The SMBIOS chassis type code as a plain word.
fn chassis_name(code: u8) -> Option<&'static str> {
    Some(match code {
        3..=7 | 15 | 16 | 24 => "Desktop",
        8 | 9 | 10 | 14 => "Laptop",
        11 => "Handheld",
        13 => "All-in-one",
        17 | 23 | 25 | 28 | 29 => "Server",
        30 => "Tablet",
        31 => "Convertible",
        32 => "Detachable",
        35 => "Mini PC",
        36 => "Stick PC",
        _ => return None,
    })
}

fn secure_boot(bytes: &[u8]) -> Option<bool> {
    bytes.get(4).map(|&value| value == 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_dropped() {
        assert_eq!(clean("Acer\n"), Some("Acer".into()));
        assert_eq!(clean("To Be Filled By O.E.M.\n"), None);
        assert_eq!(clean("Default string"), None);
        assert_eq!(clean("  \n"), None);
    }

    #[test]
    fn names_join_when_both_are_there() {
        assert_eq!(join_names(Some("ASUSTeK".into()), Some("PRIME B650".into())), Some("ASUSTeK PRIME B650".into()));
        assert_eq!(join_names(None, Some("IONIQ_RTX".into())), Some("IONIQ_RTX".into()));
        assert_eq!(join_names(None, None), None);
    }

    #[test]
    fn chassis_codes() {
        assert_eq!(chassis_name(10), Some("Laptop"));
        assert_eq!(chassis_name(3), Some("Desktop"));
        assert_eq!(chassis_name(2), None);
    }

    #[test]
    fn secure_boot_is_the_fifth_byte() {
        assert_eq!(secure_boot(&[6, 0, 0, 0, 1]), Some(true));
        assert_eq!(secure_boot(&[6, 0, 0, 0, 0]), Some(false));
        assert_eq!(secure_boot(&[6, 0]), None);
    }
}
