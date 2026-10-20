//! XDG autostart entries: what starts when you log in, and turning entries on
//! or off. Read from `~/.config/autostart` and each `$XDG_CONFIG_DIRS`
//! autostart folder, where a user file wins over a system file of the same name.
//!
//! Desktops that use systemd's xdg-autostart-generator (COSMIC, KDE, GNOME)
//! start each entry as an `app-<name>@autostart.service` user unit, so the
//! unit's state says whether it's running. The generator only runs at login,
//! so turning an entry on or off takes effect next time you log in.

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

const GROUP: &str = "[Desktop Entry]";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Only in your autostart folder.
    User,
    /// Only in a system folder like `/etc/xdg/autostart`.
    System,
    /// A system entry with your own copy on top of it.
    Override,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::User => "User",
            Source::System => "System",
            Source::Override => "System, your copy",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    /// The unit ran and finished without staying around.
    Finished,
    Starting,
    Failed,
    NotRunning,
    /// `TryExec` names a program that isn't installed, so it never starts.
    ProgramMissing,
    /// The entry is a link to a file that's gone.
    FileMissing,
}

impl Status {
    pub fn label(self) -> &'static str {
        match self {
            Status::Running => "Running",
            Status::Finished => "Finished",
            Status::Starting => "Starting",
            Status::Failed => "Failed",
            Status::NotRunning => "Not running",
            Status::ProgramMissing => "Program missing",
            Status::FileMissing => "File missing",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The file name, like `Beeper.desktop`. Unique across the list.
    pub id: String,
    pub name: String,
    pub comment: Option<String>,
    pub exec: Option<String>,
    /// The file that's in effect: your copy when there is one.
    pub path: PathBuf,
    /// Where `path` points when it's a symlink.
    pub link: Option<PathBuf>,
    pub source: Source,
    pub enabled: bool,
    /// Why autostart never starts it here, like "Only on KDE".
    pub elsewhere: Option<String>,
    /// Seconds to wait after login, from `X-GNOME-Autostart-Delay`.
    pub delay: Option<u32>,
    pub terminal: bool,
    pub unit: String,
    /// Whether the generator made a unit for it at login, so it can be started now.
    pub generated: bool,
    pub status: Status,
}

impl Entry {
    /// Starts at login on this desktop.
    pub fn starts_here(&self) -> bool {
        self.enabled && self.elsewhere.is_none() && self.status != Status::FileMissing
    }

    pub fn can_start(&self) -> bool {
        self.generated && self.elsewhere.is_none() && !matches!(self.status, Status::Running | Status::Starting)
    }

    pub fn can_stop(&self) -> bool {
        matches!(self.status, Status::Running | Status::Starting)
    }
}

/// Every autostart entry, sorted by name, with unit states from `systemctl`.
pub fn read() -> Vec<Entry> {
    let desktops: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().split(':').filter(|d| !d.is_empty()).map(String::from).collect();
    let units = read_units().unwrap_or_default();
    let generated = generator_dir();
    let mut entries = read_dirs(&user_dir(), &system_dirs(), &desktops, &units, generated.as_deref());
    entries.sort_by_cached_key(|entry| entry.name.to_lowercase());
    entries
}

fn read_dirs(user: &Option<PathBuf>, system: &[PathBuf], desktops: &[String], units: &HashMap<String, (String, String)>, generated: Option<&Path>) -> Vec<Entry> {
    // file name -> (file in effect, is a user file, a system file sits under it)
    let mut found: Vec<(String, PathBuf, bool, bool)> = Vec::new();
    let dirs = user.iter().map(|dir| (dir, true)).chain(system.iter().map(|dir| (dir, false)));
    for (dir, is_user) in dirs {
        for id in desktop_files(dir) {
            match found.iter_mut().find(|(seen, ..)| *seen == id) {
                Some((_, _, true, shadows)) if !is_user => *shadows = true,
                Some(_) => {}
                None => found.push((id.clone(), dir.join(&id), is_user, false)),
            }
        }
    }
    found
        .into_iter()
        .map(|(id, path, is_user, shadows)| {
            let source = match (is_user, shadows) {
                (true, true) => Source::Override,
                (true, false) => Source::User,
                (false, _) => Source::System,
            };
            let unit = unit_name(&id);
            let generated = generated.is_some_and(|dir| dir.join(&unit).exists());
            entry(id, path, source, desktops, units.get(&unit), unit, generated)
        })
        .collect()
}

fn desktop_files(dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries.filter_map(Result::ok).filter_map(|entry| entry.file_name().into_string().ok()).filter(|name| name.ends_with(".desktop")).collect()
}

fn entry(id: String, path: PathBuf, source: Source, desktops: &[String], unit_state: Option<&(String, String)>, unit: String, generated: bool) -> Entry {
    let link = std::fs::read_link(&path).ok().map(|target| path.parent().map_or(target.clone(), |dir| dir.join(&target)));
    let stem = id.trim_end_matches(".desktop").to_string();
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Entry {
            name: stem,
            comment: None,
            exec: None,
            link,
            source,
            enabled: false,
            elsewhere: None,
            delay: None,
            terminal: false,
            generated: false,
            status: Status::FileMissing,
            id,
            path,
            unit,
        };
    };
    let keys = parse(&text);
    let get = |key: &str| keys.get(key).map(String::as_str).filter(|value| !value.is_empty());
    let truthy = |key: &str| get(key).is_some_and(|value| value.eq_ignore_ascii_case("true"));
    let enabled = !truthy("Hidden") && !get("X-GNOME-Autostart-enabled").is_some_and(|value| value.eq_ignore_ascii_case("false"));
    let status = if get("TryExec").is_some_and(|program| find_program(program).is_none()) {
        Status::ProgramMissing
    } else {
        unit_state.map_or(Status::NotRunning, |(active, sub)| unit_status(active, sub))
    };
    Entry {
        name: get("Name").unwrap_or(&stem).to_string(),
        comment: get("Comment").map(String::from),
        exec: get("Exec").map(String::from),
        link,
        source,
        enabled,
        elsewhere: elsewhere(&keys, desktops),
        delay: get("X-GNOME-Autostart-Delay").and_then(|delay| delay.parse().ok()).filter(|&delay| delay > 0),
        terminal: truthy("Terminal"),
        generated,
        status,
        id,
        path,
        unit,
    }
}

/// Keys of the `[Desktop Entry]` group. Translated keys like `Name[fr]` are skipped.
fn parse(text: &str) -> HashMap<String, String> {
    let mut keys = HashMap::new();
    let mut in_group = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_group = line == GROUP;
            continue;
        }
        if !in_group || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if !key.contains('[') {
                keys.entry(key.to_string()).or_insert_with(|| value.trim().to_string());
            }
        }
    }
    keys
}

/// Why an entry never starts on the current desktop, or `None` when it does.
fn elsewhere(keys: &HashMap<String, String>, desktops: &[String]) -> Option<String> {
    let list = |key: &str| keys.get(key).map(|value| value.split(';').filter(|d| !d.is_empty()).map(String::from).collect::<Vec<_>>());
    let here = |names: &[String]| names.iter().any(|name| desktops.contains(name));
    if let Some(only) = list("OnlyShowIn").filter(|only| !here(only)) {
        return Some(format!("Only on {}", only.join(", ")));
    }
    if let Some(not) = list("NotShowIn").filter(|not| here(not)) {
        let current = desktops.iter().find(|desktop| not.contains(desktop)).cloned().unwrap_or_default();
        return Some(format!("Not on {current}"));
    }
    // systemd leaves these to gnome-session, so nothing else starts them.
    if keys.contains_key("X-GNOME-Autostart-Phase") && !desktops.iter().any(|desktop| desktop == "GNOME") {
        return Some("Only in GNOME sessions".into());
    }
    // systemd skips these because a unit or session manager of their own starts them.
    if keys.get("X-systemd-skip").is_some_and(|value| value.eq_ignore_ascii_case("true")) {
        return Some("Started another way".into());
    }
    None
}

fn unit_status(active: &str, sub: &str) -> Status {
    match (active, sub) {
        ("active", "running") => Status::Running,
        ("active", _) => Status::Finished,
        ("activating" | "reloading", _) => Status::Starting,
        ("failed", _) => Status::Failed,
        _ => Status::NotRunning,
    }
}

/// The unit systemd's generator makes for a file name, like
/// `app-arch\x2dupdate\x2dtray@autostart.service`.
pub fn unit_name(id: &str) -> String {
    let stem = id.strip_suffix(".desktop").unwrap_or(id);
    let mut escaped = String::with_capacity(stem.len());
    for (ix, byte) in stem.bytes().enumerate() {
        match byte {
            b'.' if ix == 0 => escaped.push_str("\\x2e"),
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b':' | b'_' | b'.' => escaped.push(byte as char),
            b'/' => escaped.push('-'),
            _ => escaped.push_str(&format!("\\x{byte:02x}")),
        }
    }
    format!("app-{escaped}@autostart.service")
}

#[derive(Deserialize)]
struct UnitRow {
    unit: String,
    active: String,
    sub: String,
}

/// Loaded autostart units: name -> (active state, sub state).
fn read_units() -> Option<HashMap<String, (String, String)>> {
    let output = Command::new("systemctl")
        .args(["--user", "list-units", "--all", "--no-pager", "--output=json", "app-*@autostart.service"])
        .output()
        .ok()?;
    let rows: Vec<UnitRow> = serde_json::from_slice(&output.stdout).ok()?;
    Some(rows.into_iter().map(|row| (row.unit, (row.active, row.sub))).collect())
}

fn generator_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join("systemd/generator.late"))
}

fn user_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .map(|dir| dir.join("autostart"))
}

fn system_dirs() -> Vec<PathBuf> {
    let dirs = std::env::var("XDG_CONFIG_DIRS").ok().filter(|dirs| !dirs.is_empty()).unwrap_or_else(|| "/etc/xdg".into());
    dirs.split(':').filter(|dir| !dir.is_empty()).map(|dir| Path::new(dir).join("autostart")).collect()
}

/// A program on `PATH`, or a path that exists.
pub fn find_program(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let path = PathBuf::from(program);
        return path.exists().then_some(path);
    }
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|dir| dir.join(program)).find(|path| path.is_file()))
}

/// Turns an entry on or off by writing `Hidden=` into your copy of it, which
/// is made first for a system entry. A symlinked entry is replaced by a real
/// file, so the app launcher it points to is never changed.
pub fn set_enabled(entry: &Entry, enabled: bool) -> io::Result<()> {
    let dir = user_dir().ok_or_else(|| io::Error::other("HOME isn't set"))?;
    set_enabled_in(entry, enabled, &dir)
}

fn set_enabled_in(entry: &Entry, enabled: bool, dir: &Path) -> io::Result<()> {
    let text = std::fs::read_to_string(&entry.path)?;
    let mut text = set_key(&text, "Hidden", if enabled { "false" } else { "true" }).ok_or_else(|| io::Error::other("It isn't a desktop entry"))?;
    if enabled && parse(&text).get("X-GNOME-Autostart-enabled").is_some_and(|value| value.eq_ignore_ascii_case("false")) {
        text = set_key(&text, "X-GNOME-Autostart-enabled", "true").unwrap_or(text);
    }
    std::fs::create_dir_all(dir)?;
    // Write beside it and rename over it, which replaces a link instead of following it.
    let temp = dir.join(format!(".{}.purrtop", entry.id));
    std::fs::write(&temp, text)?;
    std::fs::rename(&temp, dir.join(&entry.id)).inspect_err(|_| _ = std::fs::remove_file(&temp))
}

/// Sets `key=value` in the `[Desktop Entry]` group, replacing the key if it's
/// there and adding it at the end of the group if not. Everything else stays as it was.
fn set_key(text: &str, key: &str, value: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.iter().position(|line| line.trim() == GROUP)?;
    let end = lines[start + 1..].iter().position(|line| line.trim_start().starts_with('[')).map_or(lines.len(), |ix| start + 1 + ix);
    let setting = format!("{key}={value}");
    let mut out: Vec<String> = lines.iter().map(|line| line.to_string()).collect();
    let existing = (start + 1..end).find(|&ix| lines[ix].split_once('=').is_some_and(|(name, _)| name.trim() == key));
    match existing {
        Some(ix) => out[ix] = setting,
        None => {
            // After the group's last non-blank line, so a blank line before the next group stays put.
            let last = (start..end).rev().find(|&ix| !lines[ix].trim().is_empty()).unwrap_or(start);
            out.insert(last + 1, setting);
        }
    }
    let mut joined = out.join("\n");
    joined.push('\n');
    Some(joined)
}

/// Starts or stops the entry's unit now. Blocks; call it off the UI thread.
pub fn start(entry: &Entry, start: bool) -> Result<(), String> {
    let output = Command::new("systemctl")
        .args(["--user", if start { "start" } else { "stop" }, &entry.unit])
        .output()
        .map_err(|err| format!("Couldn't run systemctl: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(stderr.lines().next().unwrap_or("systemctl failed").trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEEPER: &str = "[Desktop Entry]\nName=Beeper\nName[de]=Bieper\nExec=/opt/Beeper.AppImage %u\nType=Application\n\n[Desktop Action New]\nName=New window\n";

    fn desktops(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn parses_only_the_main_group() {
        let keys = parse(BEEPER);
        assert_eq!(keys.get("Name").map(String::as_str), Some("Beeper"));
        assert_eq!(keys.get("Exec").map(String::as_str), Some("/opt/Beeper.AppImage %u"));
        assert!(!keys.contains_key("Name[de]"));
    }

    #[test]
    fn escapes_unit_names_like_systemd() {
        assert_eq!(unit_name("arch-update-tray.desktop"), "app-arch\\x2dupdate\\x2dtray@autostart.service");
        assert_eq!(unit_name("com.github.m0rf30.CosmicExtQuakeTerminal.desktop"), "app-com.github.m0rf30.CosmicExtQuakeTerminal@autostart.service");
        assert_eq!(
            unit_name("appimagekit_115711abc3c8ec3c99ef0f330db5e4d8-Command_Center.desktop"),
            "app-appimagekit_115711abc3c8ec3c99ef0f330db5e4d8\\x2dCommand_Center@autostart.service"
        );
    }

    #[test]
    fn knows_which_desktop_an_entry_is_for() {
        let keys = |text: &str| parse(&format!("[Desktop Entry]\n{text}\n"));
        let cosmic = desktops(&["COSMIC"]);
        assert_eq!(elsewhere(&keys("OnlyShowIn=KDE;"), &cosmic), Some("Only on KDE".into()));
        assert_eq!(elsewhere(&keys("OnlyShowIn=KDE;COSMIC;"), &cosmic), None);
        assert_eq!(elsewhere(&keys("NotShowIn=KDE;COSMIC;"), &cosmic), Some("Not on COSMIC".into()));
        assert_eq!(elsewhere(&keys("NotShowIn=GNOME;"), &cosmic), None);
        assert_eq!(elsewhere(&keys("X-GNOME-Autostart-Phase=Initialization"), &cosmic), Some("Only in GNOME sessions".into()));
        assert_eq!(elsewhere(&keys("X-GNOME-Autostart-Phase=Initialization"), &desktops(&["ubuntu", "GNOME"])), None);
        assert_eq!(elsewhere(&keys("X-systemd-skip=true"), &cosmic), Some("Started another way".into()));
    }

    #[test]
    fn sets_keys_without_touching_the_rest() {
        let hidden = set_key(BEEPER, "Hidden", "true").unwrap();
        assert_eq!(hidden, "[Desktop Entry]\nName=Beeper\nName[de]=Bieper\nExec=/opt/Beeper.AppImage %u\nType=Application\nHidden=true\n\n[Desktop Action New]\nName=New window\n");
        let shown = set_key(&hidden, "Hidden", "false").unwrap();
        assert_eq!(shown.matches("Hidden=").count(), 1);
        assert!(shown.contains("Hidden=false\n\n[Desktop Action New]"));
        assert_eq!(set_key("not a desktop file", "Hidden", "true"), None);
    }

    #[test]
    fn maps_unit_states() {
        assert_eq!(unit_status("active", "running"), Status::Running);
        assert_eq!(unit_status("active", "exited"), Status::Finished);
        assert_eq!(unit_status("failed", "failed"), Status::Failed);
        assert_eq!(unit_status("inactive", "dead"), Status::NotRunning);
    }

    /// A scratch folder holding a "system" and a "user" autostart folder.
    fn scratch(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("purrtop-autostart-{name}-{}", std::process::id()));
        _ = std::fs::remove_dir_all(&root);
        let (user, system) = (root.join("user"), root.join("system"));
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        (root, user, system)
    }

    fn read_scratch(user: &Path, system: &Path) -> Vec<Entry> {
        read_dirs(&Some(user.to_path_buf()), &[system.to_path_buf()], &desktops(&["COSMIC"]), &HashMap::new(), None)
    }

    #[test]
    fn user_files_win_and_disabling_a_system_entry_makes_a_copy() {
        let (root, user, system) = scratch("override");
        std::fs::write(system.join("tray.desktop"), "[Desktop Entry]\nName=Tray\nExec=tray\n").unwrap();
        let entries = read_scratch(&user, &system);
        assert_eq!(entries.len(), 1);
        assert_eq!((entries[0].source, entries[0].enabled), (Source::System, true));

        set_enabled_in(&entries[0], false, &user).unwrap();
        let entries = read_scratch(&user, &system);
        assert_eq!((entries[0].source, entries[0].enabled), (Source::Override, false));
        assert_eq!(std::fs::read_to_string(system.join("tray.desktop")).unwrap(), "[Desktop Entry]\nName=Tray\nExec=tray\n");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn disabling_a_link_leaves_the_launcher_alone() {
        let (root, user, system) = scratch("link");
        let launcher = root.join("app.desktop");
        std::fs::write(&launcher, "[Desktop Entry]\nName=App\nExec=app\n").unwrap();
        std::os::unix::fs::symlink(&launcher, user.join("app.desktop")).unwrap();
        std::os::unix::fs::symlink(root.join("gone.desktop"), user.join("gone.desktop")).unwrap();

        let entries = read_scratch(&user, &system);
        let app = entries.iter().find(|entry| entry.id == "app.desktop").unwrap();
        assert_eq!(app.link.as_deref(), Some(launcher.as_path()));
        let gone = entries.iter().find(|entry| entry.id == "gone.desktop").unwrap();
        assert_eq!(gone.status, Status::FileMissing);

        set_enabled_in(app, false, &user).unwrap();
        assert_eq!(std::fs::read_to_string(&launcher).unwrap(), "[Desktop Entry]\nName=App\nExec=app\n");
        assert!(std::fs::read_link(user.join("app.desktop")).is_err(), "the link became a real file");
        let entries = read_scratch(&user, &system);
        assert!(!entries.iter().find(|entry| entry.id == "app.desktop").unwrap().enabled);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn enabling_also_clears_the_gnome_switch() {
        let (root, user, system) = scratch("gnome");
        std::fs::write(user.join("old.desktop"), "[Desktop Entry]\nName=Old\nExec=old\nX-GNOME-Autostart-enabled=false\n").unwrap();
        let entries = read_scratch(&user, &system);
        assert!(!entries[0].enabled);
        set_enabled_in(&entries[0], true, &user).unwrap();
        assert!(read_scratch(&user, &system)[0].enabled);
        std::fs::remove_dir_all(root).unwrap();
    }
}
