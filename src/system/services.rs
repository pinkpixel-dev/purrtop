//! systemd services over D-Bus: listing them, watching for changes, and
//! starting, stopping, enabling and disabling them.
//!
//! System units go through the system bus, where systemd checks each action
//! with polkit. Calls are flagged to allow interactive auth, so the desktop's
//! polkit agent asks for a password when one is needed.

use std::collections::HashMap;
use std::process::Command;

use zbus::proxy::CacheProperties;
use zbus::zvariant::OwnedObjectPath;
use zbus::{Connection, MatchRule, MessageStream, proxy};

const DESTINATION: &str = "org.freedesktop.systemd1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Scope {
    #[default]
    System,
    User,
}

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::System => "System",
            Scope::User => "User",
        }
    }
}

/// One row of `ListUnitsByPatterns`: name, description, load, active, sub,
/// followed, path, job ID, job type, job path.
type UnitRow = (String, String, String, String, String, String, OwnedObjectPath, u32, String, OwnedObjectPath);
/// What enabling or disabling changed: kind, link and target per file.
type Changes = Vec<(String, String, String)>;

#[proxy(interface = "org.freedesktop.systemd1.Manager", default_service = "org.freedesktop.systemd1", default_path = "/org/freedesktop/systemd1")]
trait Manager {
    fn list_units_by_patterns(&self, states: &[&str], patterns: &[&str]) -> zbus::Result<Vec<UnitRow>>;
    fn list_unit_files_by_patterns(&self, states: &[&str], patterns: &[&str]) -> zbus::Result<Vec<(String, String)>>;
    fn get_unit(&self, name: &str) -> zbus::Result<OwnedObjectPath>;
    fn subscribe(&self) -> zbus::Result<()>;
    #[zbus(allow_interactive_auth)]
    fn start_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    #[zbus(allow_interactive_auth)]
    fn stop_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    #[zbus(allow_interactive_auth)]
    fn restart_unit(&self, name: &str, mode: &str) -> zbus::Result<OwnedObjectPath>;
    #[zbus(allow_interactive_auth)]
    fn enable_unit_files(&self, files: &[&str], runtime: bool, force: bool) -> zbus::Result<(bool, Changes)>;
    #[zbus(allow_interactive_auth)]
    fn disable_unit_files(&self, files: &[&str], runtime: bool) -> zbus::Result<Changes>;
    #[zbus(allow_interactive_auth)]
    fn reload(&self) -> zbus::Result<()>;
}

#[proxy(interface = "org.freedesktop.systemd1.Service", default_service = "org.freedesktop.systemd1")]
trait ServiceUnit {
    #[zbus(property, name = "MainPID")]
    fn main_pid(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn memory_current(&self) -> zbus::Result<u64>;
}

#[proxy(interface = "org.freedesktop.systemd1.Unit", default_service = "org.freedesktop.systemd1")]
trait Unit {
    #[zbus(property)]
    fn fragment_path(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn active_enter_timestamp(&self) -> zbus::Result<u64>;
}

/// What a service is doing right now, from its active and sub states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    Failed,
    Running,
    Starting,
    Stopping,
    /// Ran and finished, like a oneshot that stays "active".
    Exited,
    Inactive,
}

impl State {
    fn parse(active: &str, sub: &str) -> Self {
        match (active, sub) {
            ("failed", _) => State::Failed,
            ("active", "exited") => State::Exited,
            ("active" | "reloading" | "refreshing", _) => State::Running,
            ("activating", _) => State::Starting,
            ("deactivating", _) => State::Stopping,
            _ => State::Inactive,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            State::Failed => "Failed",
            State::Running => "Running",
            State::Starting => "Starting",
            State::Stopping => "Stopping",
            State::Exited => "Exited",
            State::Inactive => "Stopped",
        }
    }

    pub fn is_active(self) -> bool {
        matches!(self, State::Running | State::Starting | State::Stopping | State::Exited)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Service {
    pub name: String,
    pub description: String,
    pub state: State,
    /// The raw sub state, like `running`, `dead` or `auto-restart`.
    pub sub: String,
    /// `enabled`, `disabled`, `static`, `masked` and so on. `None` for
    /// units with no unit file (transient or generated on the fly).
    pub startup: Option<String>,
    pub main_pid: Option<u32>,
    pub memory: Option<u64>,
}

impl Service {
    pub fn is_enabled(&self) -> bool {
        matches!(self.startup.as_deref(), Some("enabled" | "enabled-runtime" | "alias" | "indirect" | "linked" | "linked-runtime"))
    }

    pub fn can_enable(&self) -> bool {
        matches!(self.startup.as_deref(), Some("disabled" | "indirect"))
    }

    pub fn can_disable(&self) -> bool {
        matches!(self.startup.as_deref(), Some("enabled" | "enabled-runtime" | "linked" | "linked-runtime"))
    }

    pub fn is_masked(&self) -> bool {
        self.startup.as_deref().is_some_and(|startup| startup.starts_with("masked"))
    }

    /// Stopped and not set to start on its own, so it's folded away.
    pub fn is_idle(&self) -> bool {
        self.state == State::Inactive && !self.is_enabled()
    }

    pub fn startup_label(&self) -> String {
        let Some(startup) = self.startup.as_deref() else {
            return "—".into();
        };
        let mut chars = startup.chars();
        chars.next().map(|first| first.to_uppercase().chain(chars).collect()).unwrap_or_default()
    }
}

/// The selected service's extra details.
#[derive(Clone, Debug, Default)]
pub struct Details {
    pub path: Option<String>,
    /// Microseconds since the Unix epoch.
    pub active_since: Option<u64>,
    /// The last few journal lines, oldest first.
    pub log: Vec<String>,
}

/// A connection to one systemd instance: the system one or your user one.
#[derive(Clone)]
pub struct Systemd {
    scope: Scope,
    connection: Connection,
    manager: ManagerProxy<'static>,
}

impl Systemd {
    pub async fn connect(scope: Scope) -> zbus::Result<Self> {
        let connection = match scope {
            Scope::System => Connection::system().await?,
            Scope::User => Connection::session().await?,
        };
        let manager = ManagerProxy::builder(&connection).cache_properties(CacheProperties::No).build().await?;
        // Without a subscriber, systemd doesn't send per-unit change signals.
        manager.subscribe().await?;
        Ok(Self { scope, connection, manager })
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// Every service: the loaded ones plus unit files that aren't loaded.
    /// Running ones also get their main PID and memory.
    pub async fn list(&self) -> zbus::Result<Vec<Service>> {
        let units = self.manager.list_units_by_patterns(&[], &["*.service"]).await?;
        let files = self.manager.list_unit_files_by_patterns(&[], &["*.service"]).await?;
        let mut services = merge(units, files);
        for service in services.iter_mut().filter(|service| service.state.is_active() && service.state != State::Exited) {
            let Ok(proxy) = self.service_proxy(&service.name).await else {
                continue;
            };
            service.main_pid = proxy.main_pid().await.ok().filter(|&pid| pid > 0);
            service.memory = proxy.memory_current().await.ok().filter(|&bytes| bytes != u64::MAX);
        }
        Ok(services)
    }

    async fn service_proxy(&self, name: &str) -> zbus::Result<ServiceUnitProxy<'static>> {
        let path = self.manager.get_unit(name).await?;
        ServiceUnitProxy::builder(&self.connection).path(path)?.cache_properties(CacheProperties::No).build().await
    }

    /// Every signal systemd sends: unit changes, jobs finishing, unit files changing.
    pub async fn changes(&self) -> zbus::Result<MessageStream> {
        let rule = MatchRule::builder().msg_type(zbus::message::Type::Signal).sender(DESTINATION)?.build();
        MessageStream::for_match_rule(rule, &self.connection, Some(64)).await
    }

    pub async fn details(&self, name: &str) -> Details {
        let mut details = Details::default();
        if let Ok(path) = self.manager.get_unit(name).await
            && let Ok(builder) = UnitProxy::builder(&self.connection).path(path)
            && let Ok(unit) = builder.cache_properties(CacheProperties::No).build().await
        {
            details.path = unit.fragment_path().await.ok().filter(|path| !path.is_empty());
            details.active_since = unit.active_enter_timestamp().await.ok().filter(|&micros| micros > 0);
        }
        details
    }

    pub async fn start(&self, name: &str) -> Result<(), String> {
        self.manager.start_unit(name, "replace").await.map(|_| ()).map_err(message)
    }

    pub async fn stop(&self, name: &str) -> Result<(), String> {
        self.manager.stop_unit(name, "replace").await.map(|_| ()).map_err(message)
    }

    pub async fn restart(&self, name: &str) -> Result<(), String> {
        self.manager.restart_unit(name, "replace").await.map(|_| ()).map_err(message)
    }

    /// Enables or disables the unit file, then reloads systemd like
    /// `systemctl enable` does, so the change shows up right away.
    pub async fn set_enabled(&self, name: &str, enabled: bool) -> Result<(), String> {
        if enabled {
            self.manager.enable_unit_files(&[name], false, false).await.map_err(message)?;
        } else {
            self.manager.disable_unit_files(&[name], false).await.map_err(message)?;
        }
        self.manager.reload().await.map_err(message)
    }
}

/// The last `lines` journal lines for a unit, oldest first. Blocks; run it
/// off the UI thread.
pub fn journal(scope: Scope, name: &str, lines: usize) -> Vec<String> {
    let unit_flag = match scope {
        Scope::System => "--unit",
        Scope::User => "--user-unit",
    };
    let Ok(output) = Command::new("journalctl")
        .args([unit_flag, name, "--lines", &lines.to_string(), "--output", "short", "--no-pager", "--no-hostname", "--quiet"])
        .output()
    else {
        return Vec::new();
    };
    String::from_utf8_lossy(&output.stdout).lines().map(str::to_string).collect()
}

/// D-Bus errors carry a readable description; use it when there is one.
fn message(err: zbus::Error) -> String {
    match &err {
        zbus::Error::MethodError(name, description, _) => match name.as_str() {
            "org.freedesktop.DBus.Error.AccessDenied" | "org.freedesktop.DBus.Error.InteractiveAuthorizationRequired" => {
                "Not allowed (authentication was cancelled or failed)".into()
            }
            _ => description.clone().unwrap_or_else(|| name.to_string()),
        },
        _ => err.to_string(),
    }
}

/// Joins loaded units with unit files. Templates (`foo@.service`) only show
/// through their instances, and their instances take the template's state.
fn merge(units: Vec<UnitRow>, files: Vec<(String, String)>) -> Vec<Service> {
    let states: HashMap<String, String> = files
        .into_iter()
        .filter_map(|(path, state)| Some((path.rsplit('/').next()?.to_string(), state)))
        .collect();
    let startup = |name: &str| {
        states.get(name).cloned().or_else(|| {
            let (prefix, _) = name.split_once('@')?;
            states.get(&format!("{prefix}@.service")).cloned()
        })
    };
    let mut services: Vec<Service> = units
        .into_iter()
        .filter(|unit| unit.0.ends_with(".service"))
        .map(|(name, description, _load, active, sub, ..)| Service {
            startup: startup(&name),
            state: State::parse(&active, &sub),
            sub,
            description,
            name,
            main_pid: None,
            memory: None,
        })
        .collect();
    let mut unloaded: Vec<Service> = states
        .iter()
        .filter(|(name, _)| !name.ends_with("@.service") && !services.iter().any(|service| &service.name == *name))
        .map(|(name, state)| Service {
            name: name.clone(),
            description: String::new(),
            state: State::Inactive,
            sub: "dead".into(),
            startup: Some(state.clone()),
            main_pid: None,
            memory: None,
        })
        .collect();
    services.append(&mut unloaded);
    services.sort_by(|a, b| a.name.cmp(&b.name));
    services
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(name: &str, active: &str, sub: &str) -> UnitRow {
        let root = OwnedObjectPath::try_from("/").unwrap();
        (name.into(), format!("{name} description"), "loaded".into(), active.into(), sub.into(), String::new(), root.clone(), 0, String::new(), root)
    }

    #[test]
    fn reads_states() {
        assert_eq!(State::parse("active", "running"), State::Running);
        assert_eq!(State::parse("active", "exited"), State::Exited);
        assert_eq!(State::parse("failed", "failed"), State::Failed);
        assert_eq!(State::parse("activating", "auto-restart"), State::Starting);
        assert_eq!(State::parse("inactive", "dead"), State::Inactive);
        assert!(State::Exited.is_active() && !State::Failed.is_active());
    }

    #[test]
    fn merges_units_with_unit_files() {
        let units = vec![unit("sshd.service", "active", "running"), unit("getty@tty1.service", "active", "running"), unit("tmp.mount", "active", "mounted")];
        let files = vec![
            ("/usr/lib/systemd/system/sshd.service".into(), "enabled".into()),
            ("/usr/lib/systemd/system/getty@.service".into(), "enabled".into()),
            ("/usr/lib/systemd/system/cups.service".into(), "disabled".into()),
        ];
        let services = merge(units, files);
        let names: Vec<&str> = services.iter().map(|service| service.name.as_str()).collect();
        // Not the mount, not the template itself, plus the unloaded cups.
        assert_eq!(names, ["cups.service", "getty@tty1.service", "sshd.service"]);
        let cups = &services[0];
        assert_eq!((cups.state, cups.startup.as_deref()), (State::Inactive, Some("disabled")));
        assert!(cups.is_idle() && cups.can_enable() && !cups.can_disable());
        // Instances take the template's state.
        assert_eq!(services[1].startup.as_deref(), Some("enabled"));
        assert_eq!(services[2].startup_label(), "Enabled");
        assert!(services[2].can_disable() && !services[2].is_idle());
    }

    #[test]
    fn labels_units_without_files() {
        let services = merge(vec![unit("run-u42.service", "active", "running")], Vec::new());
        assert_eq!(services[0].startup, None);
        assert_eq!(services[0].startup_label(), "—");
        assert!(!services[0].can_enable() && !services[0].can_disable());
    }

    #[test]
    fn lists_this_machine() {
        // CI containers often have no system bus; only check a live read when there is one.
        let Ok(systemd) = zbus::block_on(Systemd::connect(Scope::System)) else {
            return;
        };
        let services = zbus::block_on(systemd.list()).expect("ListUnits should work once connected");
        assert!(services.iter().all(|service| service.name.ends_with(".service")));
        assert!(services.iter().any(|service| service.state == State::Running));
    }
}
