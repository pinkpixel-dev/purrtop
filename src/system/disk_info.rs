//! What each disk is (model, size, kind) and the volumes mounted from it.

use std::ffi::CString;
use std::path::Path;

use super::sensors::read_u64;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiskKind {
    Nvme,
    Ssd,
    Hdd,
    #[default]
    Unknown,
}

impl DiskKind {
    pub fn label(self) -> &'static str {
        match self {
            DiskKind::Nvme => "NVMe SSD",
            DiskKind::Ssd => "SSD",
            DiskKind::Hdd => "HDD",
            DiskKind::Unknown => "Disk",
        }
    }
}

/// Facts about a disk that don't change while it's plugged in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiskInfo {
    pub model: Option<String>,
    /// Bytes.
    pub size: u64,
    pub kind: DiskKind,
    pub removable: bool,
}

impl DiskInfo {
    pub fn read(name: &str) -> Self {
        let dir = Path::new("/sys/block").join(name);
        let model = std::fs::read_to_string(dir.join("device/model")).ok().map(|model| model.trim().to_string()).filter(|model| !model.is_empty());
        let rotational = read_u64(&dir.join("queue/rotational"));
        let kind = match (name.starts_with("nvme"), rotational) {
            (true, _) => DiskKind::Nvme,
            (false, Some(0)) => DiskKind::Ssd,
            (false, Some(_)) => DiskKind::Hdd,
            (false, None) => DiskKind::Unknown,
        };
        Self {
            model,
            // `size` counts 512-byte sectors whatever the hardware uses.
            size: read_u64(&dir.join("size")).unwrap_or(0) * 512,
            kind,
            removable: read_u64(&dir.join("removable")) == Some(1),
        }
    }
}

/// A mounted filesystem on one of the disks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Volume {
    /// The whole disk it lives on, like `nvme0n1`.
    pub disk: String,
    /// The partition or mapped device, like `nvme0n1p2`.
    pub device: String,
    pub mount: String,
    pub filesystem: String,
    pub total: u64,
    pub available: u64,
}

impl Volume {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    pub fn used_fraction(&self) -> f32 {
        if self.total == 0 { 0. } else { self.used() as f32 / self.total as f32 }
    }
}

/// Every mounted filesystem that lives on one of `disks`, with its space.
pub fn read_volumes(disks: &[String]) -> Vec<Volume> {
    let Ok(text) = std::fs::read_to_string("/proc/mounts") else {
        return Vec::new();
    };
    parse_mounts(&text)
        .into_iter()
        .filter_map(|(source, mount, filesystem)| {
            let device = device_name(&source)?;
            let disk = disk_of(&device, disks)?;
            let (total, available) = space(&mount)?;
            Some(Volume { disk, device, mount, filesystem, total, available })
        })
        .collect()
}

/// `(source, mount point, filesystem)` for every `/dev` mount. A point that's
/// listed twice is kept once.
fn parse_mounts(text: &str) -> Vec<(String, String, String)> {
    let mut mounts: Vec<(String, String, String)> = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(source), Some(mount), Some(filesystem)) = (fields.next(), fields.next(), fields.next()) else {
            continue;
        };
        if !source.starts_with("/dev/") {
            continue;
        }
        let mount = unescape(mount);
        if mounts.iter().any(|(_, seen, _)| *seen == mount) {
            continue;
        }
        mounts.push((source.to_string(), mount, filesystem.to_string()));
    }
    mounts
}

/// `/proc/mounts` writes spaces, tabs and backslashes as octal escapes.
fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut ix = 0;
    while ix < bytes.len() {
        let octal = bytes.get(ix + 1..ix + 4).and_then(|digits| std::str::from_utf8(digits).ok()).and_then(|digits| u8::from_str_radix(digits, 8).ok());
        match (bytes[ix], octal) {
            (b'\\', Some(byte)) => {
                out.push(byte);
                ix += 4;
            }
            (byte, _) => {
                out.push(byte);
                ix += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The kernel name behind a mount source: `/dev/mapper/root` becomes `dm-0`.
fn device_name(source: &str) -> Option<String> {
    let resolved = std::fs::canonicalize(source).ok()?;
    resolved.file_name()?.to_str().map(str::to_string)
}

/// Which of `disks` a device sits on: the disk itself, one of its partitions,
/// or a device-mapper device (LUKS, LVM) built on one of those.
fn disk_of(device: &str, disks: &[String]) -> Option<String> {
    disk_of_depth(device, disks, 0)
}

fn disk_of_depth(device: &str, disks: &[String], depth: usize) -> Option<String> {
    if let Some(disk) = disks.iter().find(|disk| *disk == device || Path::new("/sys/block").join(disk).join(device).exists()) {
        return Some(disk.clone());
    }
    // Stacked mapper devices are a few levels deep at most.
    if depth >= 4 {
        return None;
    }
    let slaves = std::fs::read_dir(Path::new("/sys/block").join(device).join("slaves")).ok()?;
    slaves.filter_map(Result::ok).find_map(|slave| disk_of_depth(slave.file_name().to_str()?, disks, depth + 1))
}

/// Total and available bytes on the filesystem at `mount`.
fn space(mount: &str) -> Option<(u64, u64)> {
    let path = CString::new(mount).ok()?;
    let mut stats: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `path` is a valid C string and `stats` is a writable statvfs.
    if unsafe { libc::statvfs(path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    let block = stats.f_frsize as u64;
    Some((stats.f_blocks as u64 * block, stats.f_bavail as u64 * block))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_dev_mounts_once_each() {
        let text = "proc /proc proc rw 0 0\n\
                    /dev/nvme0n1p2 / ext4 rw,noatime 0 0\n\
                    /dev/nvme0n1p1 /boot vfat rw 0 0\n\
                    /dev/sdb1 /run/media/me/My\\040Drive exfat rw 0 0\n\
                    /dev/nvme0n1p2 / ext4 rw 0 0\n";
        let mounts = parse_mounts(text);
        assert_eq!(mounts.len(), 3);
        assert_eq!(mounts[0], ("/dev/nvme0n1p2".into(), "/".into(), "ext4".into()));
        assert_eq!(mounts[2].1, "/run/media/me/My Drive");
    }

    #[test]
    fn unescapes_octal_and_leaves_the_rest() {
        assert_eq!(unescape("a\\040b\\011c"), "a b\tc");
        assert_eq!(unescape("plain\\x"), "plain\\x");
    }

    #[test]
    fn volume_math() {
        let volume = Volume { total: 400, available: 100, ..Volume::default() };
        assert_eq!(volume.used(), 300);
        assert_eq!(volume.used_fraction(), 0.75);
        assert_eq!(Volume::default().used_fraction(), 0.);
    }
}
