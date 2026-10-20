//! What the machine runs: distro, kernel, desktop session and installed
//! packages, read once.

use std::ffi::CStr;
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SoftwareInfo {
    pub os: Option<String>,
    pub os_version: Option<String>,
    pub kernel: Option<String>,
    /// The kernel's build string, like `#1 SMP PREEMPT_DYNAMIC …`.
    pub kernel_build: Option<String>,
    pub architecture: Option<String>,
    pub hostname: Option<String>,
    pub desktop: Option<String>,
    pub session: Option<String>,
    pub init: Option<String>,
    pub shell: Option<String>,
    pub locale: Option<String>,
    pub time_zone: Option<String>,
    /// Package manager and how many packages it has installed.
    pub packages: Vec<(&'static str, usize)>,
}

impl SoftwareInfo {
    pub fn read() -> Self {
        let os_release = std::fs::read_to_string("/etc/os-release")
            .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
            .unwrap_or_default();
        let field = |key| os_release_field(&os_release, key);
        let uname = uname();
        let env = |key| std::env::var(key).ok().filter(|value| !value.is_empty());
        Self {
            os: field("PRETTY_NAME").or_else(|| field("NAME")),
            os_version: field("VERSION").or_else(|| field("VERSION_ID")).or_else(|| field("BUILD_ID")),
            kernel: uname.as_ref().map(|uname| uname.release.clone()),
            kernel_build: uname.as_ref().map(|uname| uname.version.clone()),
            architecture: uname.as_ref().map(|uname| uname.machine.clone()),
            hostname: uname.map(|uname| uname.nodename),
            desktop: env("XDG_CURRENT_DESKTOP").map(|desktop| desktop.replace(':', ", ")),
            session: env("XDG_SESSION_TYPE").map(|session| session_name(&session)),
            init: std::fs::read_to_string("/proc/1/comm").ok().map(|comm| comm.trim().to_string()),
            shell: env("SHELL").map(|shell| shell.rsplit('/').next().unwrap_or(&shell).to_string()),
            locale: env("LC_ALL").or_else(|| env("LANG")),
            time_zone: time_zone(),
            packages: packages(),
        }
    }
}

/// A value from os-release, with its quotes taken off.
fn os_release_field(text: &str, key: &str) -> Option<String> {
    let value = text.lines().find_map(|line| line.strip_prefix(key)?.strip_prefix('='))?;
    let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
    (!value.is_empty()).then(|| value.to_string())
}

fn session_name(session: &str) -> String {
    match session {
        "wayland" => "Wayland".into(),
        "x11" => "X11".into(),
        "tty" => "Console".into(),
        other => other.to_string(),
    }
}

struct Uname {
    release: String,
    version: String,
    machine: String,
    nodename: String,
}

fn uname() -> Option<Uname> {
    // SAFETY: uname fills the struct in, and each field is NUL-terminated.
    unsafe {
        let mut raw: libc::utsname = std::mem::zeroed();
        if libc::uname(&mut raw) != 0 {
            return None;
        }
        let field = |chars: &[libc::c_char]| CStr::from_ptr(chars.as_ptr()).to_string_lossy().into_owned();
        Some(Uname {
            release: field(&raw.release),
            version: field(&raw.version),
            machine: field(&raw.machine),
            nodename: field(&raw.nodename),
        })
    }
}

/// `/etc/localtime` points into the zoneinfo folder, so its target names the zone.
fn time_zone() -> Option<String> {
    let link = std::fs::read_link("/etc/localtime").ok();
    let from_link = link.as_ref().and_then(|link| zone_from_link(&link.to_string_lossy()));
    from_link.or_else(|| {
        let zone = std::fs::read_to_string("/etc/timezone").ok()?;
        let zone = zone.trim();
        (!zone.is_empty()).then(|| zone.to_string())
    })
}

fn zone_from_link(link: &str) -> Option<String> {
    let (_, zone) = link.split_once("zoneinfo/")?;
    (!zone.is_empty()).then(|| zone.to_string())
}

/// Installed package counts for the package managers that are here.
fn packages() -> Vec<(&'static str, usize)> {
    let mut packages = Vec::new();
    if let Some(count) = count_folders(Path::new("/var/lib/pacman/local")) {
        packages.push(("pacman", count));
    }
    if let Ok(status) = std::fs::read_to_string("/var/lib/dpkg/status") {
        packages.push(("dpkg", count_dpkg(&status)));
    }
    let home = std::env::var_os("HOME").map(|home| Path::new(&home).join(".local/share/flatpak/app"));
    let flatpak = [Some(Path::new("/var/lib/flatpak/app").to_path_buf()), home]
        .into_iter()
        .flatten()
        .filter_map(|dir| count_folders(&dir))
        .reduce(|a, b| a + b);
    if let Some(count) = flatpak {
        packages.push(("Flatpak", count));
    }
    packages
}

fn count_folders(dir: &Path) -> Option<usize> {
    let entries = std::fs::read_dir(dir).ok()?;
    Some(entries.filter_map(Result::ok).filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir())).count())
}

fn count_dpkg(status: &str) -> usize {
    status.lines().filter(|line| *line == "Status: install ok installed").count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OS_RELEASE: &str = "NAME=\"CachyOS Linux\"\nPRETTY_NAME=\"CachyOS\"\nID=cachyos\nBUILD_ID=rolling\n";

    #[test]
    fn os_release_fields() {
        assert_eq!(os_release_field(OS_RELEASE, "PRETTY_NAME"), Some("CachyOS".into()));
        assert_eq!(os_release_field(OS_RELEASE, "NAME"), Some("CachyOS Linux".into()));
        assert_eq!(os_release_field(OS_RELEASE, "BUILD_ID"), Some("rolling".into()));
        assert_eq!(os_release_field(OS_RELEASE, "VERSION"), None);
    }

    #[test]
    fn zone_comes_from_the_link() {
        assert_eq!(zone_from_link("/usr/share/zoneinfo/America/New_York"), Some("America/New_York".into()));
        assert_eq!(zone_from_link("../usr/share/zoneinfo/UTC"), Some("UTC".into()));
        assert_eq!(zone_from_link("/etc/somewhere"), None);
    }

    #[test]
    fn dpkg_counts_installed_only() {
        let status = "Package: a\nStatus: install ok installed\n\nPackage: b\nStatus: deinstall ok config-files\n\nPackage: c\nStatus: install ok installed\n";
        assert_eq!(count_dpkg(status), 2);
    }

    #[test]
    fn reads_this_machine() {
        let info = SoftwareInfo::read();
        assert!(info.kernel.is_some());
        assert!(info.architecture.is_some());
    }
}
