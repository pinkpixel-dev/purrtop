//! Installed apps from each `applications` folder, so one can be added to
//! autostart. Read from `$XDG_DATA_HOME` first, then each `$XDG_DATA_DIRS`
//! folder, where the first file with a name wins, like app launchers do.

use std::collections::HashSet;
use std::io;
use std::path::{Path, PathBuf};

use super::autostart::{self, find_program};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Launcher {
    /// The file name, like `firefox.desktop`.
    pub id: String,
    pub name: String,
    pub comment: Option<String>,
    pub exec: String,
    pub path: PathBuf,
}

/// Apps that show up in the app launcher on this desktop, sorted by name.
pub fn read() -> Vec<Launcher> {
    let mut launchers = read_dirs(&data_dirs(), &autostart::current_desktops());
    launchers.sort_by_cached_key(|launcher| launcher.name.to_lowercase());
    launchers
}

fn read_dirs(dirs: &[PathBuf], desktops: &[String]) -> Vec<Launcher> {
    let mut seen = HashSet::new();
    let mut launchers = Vec::new();
    for dir in dirs {
        for id in autostart::desktop_files(dir) {
            // A hidden file still hides the same name in later folders.
            if seen.insert(id.clone())
                && let Some(launcher) = launcher(id, dir, desktops)
            {
                launchers.push(launcher);
            }
        }
    }
    launchers
}

fn launcher(id: String, dir: &Path, desktops: &[String]) -> Option<Launcher> {
    let path = dir.join(&id);
    let keys = autostart::parse(&std::fs::read_to_string(&path).ok()?);
    let get = |key: &str| keys.get(key).map(String::as_str).filter(|value| !value.is_empty());
    let truthy = |key: &str| get(key).is_some_and(|value| value.eq_ignore_ascii_case("true"));
    if get("Type") != Some("Application") || truthy("NoDisplay") || truthy("Hidden") || autostart::elsewhere(&keys, desktops).is_some() {
        return None;
    }
    if get("TryExec").is_some_and(|program| find_program(program).is_none()) {
        return None;
    }
    let stem = id.trim_end_matches(".desktop").to_string();
    Some(Launcher { name: get("Name").unwrap_or(&stem).to_string(), comment: get("Comment").map(String::from), exec: get("Exec")?.to_string(), id, path })
}

fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")));
    let system = std::env::var("XDG_DATA_DIRS").ok().filter(|dirs| !dirs.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    home.into_iter().chain(system.split(':').filter(|dir| !dir.is_empty()).map(PathBuf::from)).map(|dir| dir.join("applications")).collect()
}

/// Starts an app at login by linking its launcher into your autostart folder,
/// so the entry follows the launcher when the app updates it.
pub fn add(launcher: &Launcher) -> io::Result<()> {
    let dir = autostart::user_dir().ok_or_else(|| io::Error::other("HOME isn't set"))?;
    add_in(launcher, &dir)
}

fn add_in(launcher: &Launcher, dir: &Path) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::os::unix::fs::symlink(&launcher.path, dir.join(&launcher.id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("purrtop-launchers-{name}-{}", std::process::id()));
        _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("home")).unwrap();
        std::fs::create_dir_all(root.join("system")).unwrap();
        root
    }

    #[test]
    fn lists_only_apps_the_launcher_shows() {
        let root = scratch("read");
        let (home, system) = (root.join("home"), root.join("system"));
        let app = |name: &str, extra: &str| format!("[Desktop Entry]\nType=Application\nName={name}\nExec=sh\n{extra}\n");
        std::fs::write(system.join("shown.desktop"), app("Shown", "")).unwrap();
        std::fs::write(system.join("helper.desktop"), app("Helper", "NoDisplay=true")).unwrap();
        std::fs::write(system.join("kde.desktop"), app("KDE only", "OnlyShowIn=KDE;")).unwrap();
        std::fs::write(system.join("link.desktop"), "[Desktop Entry]\nType=Link\nName=Site\nURL=https://pinkpixel.dev\n").unwrap();
        std::fs::write(system.join("gone.desktop"), app("Gone", "TryExec=/nowhere/purrtop-test")).unwrap();
        std::fs::write(system.join("hidden.desktop"), app("Hidden", "")).unwrap();
        // A hidden copy in the home folder hides the system one too.
        std::fs::write(home.join("hidden.desktop"), app("Hidden", "Hidden=true")).unwrap();

        let launchers = read_dirs(&[home, system.clone()], &["COSMIC".to_string()]);
        let names: Vec<&str> = launchers.iter().map(|launcher| launcher.name.as_str()).collect();
        assert_eq!(names, ["Shown"]);
        assert_eq!(launchers[0].path, system.join("shown.desktop"));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn adding_links_the_launcher_into_autostart() {
        let root = scratch("add");
        let path = root.join("system/app.desktop");
        std::fs::write(&path, "[Desktop Entry]\nType=Application\nName=App\nExec=sh\n").unwrap();
        let launcher = Launcher { id: "app.desktop".into(), name: "App".into(), comment: None, exec: "sh".into(), path: path.clone() };
        let autostart = root.join("autostart");
        add_in(&launcher, &autostart).unwrap();
        assert_eq!(std::fs::read_link(autostart.join("app.desktop")).unwrap(), path);
        assert_eq!(add_in(&launcher, &autostart).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        std::fs::remove_dir_all(root).unwrap();
    }
}
