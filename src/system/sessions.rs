//! Login sessions from systemd-logind, through `loginctl`, plus totals of
//! what each user is running.

use std::collections::HashMap;
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use super::processes::ProcessInfo;

/// The `show-session` properties purrtop reads, in the order it asks for them.
const PROPERTIES: [&str; 14] =
    ["Id", "User", "Name", "Class", "Type", "Desktop", "Service", "TTY", "Seat", "RemoteHost", "State", "IdleHint", "LockedHint", "TimestampMonotonic"];

/// UIDs from here up are people, not system accounts (the usual
/// `UID_MIN` to `UID_MAX` range from `/etc/login.defs`).
const PEOPLE: std::ops::Range<u32> = 1000..60000;

/// One logind session.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub uid: u32,
    pub user: String,
    /// `user`, `greeter`, `lock-screen`, and so on.
    pub class: String,
    /// `wayland`, `x11`, `tty` or `unspecified`.
    pub kind: String,
    pub desktop: Option<String>,
    pub service: Option<String>,
    pub tty: Option<String>,
    pub seat: Option<String>,
    pub remote_host: Option<String>,
    /// `active`, `online` or `closing`.
    pub state: String,
    pub idle: bool,
    pub locked: bool,
    /// When the session started, counted from boot.
    pub started: Option<Duration>,
}

impl Session {
    /// "Wayland · COSMIC · tty1", or "SSH from 10.0.0.2".
    pub fn label(&self) -> String {
        let kind = match self.kind.as_str() {
            "wayland" => "Wayland",
            "x11" => "X11",
            "tty" => "Terminal",
            "mir" => "Mir",
            _ => "Session",
        };
        let mut parts = vec![kind.to_string()];
        parts.extend(self.desktop.clone());
        parts.extend(self.tty.clone());
        if let Some(host) = &self.remote_host {
            parts.push(format!("from {host}"));
        }
        parts.join(" · ")
    }

    pub fn status(&self) -> &'static str {
        match (self.locked, self.idle, self.state.as_str()) {
            (true, ..) => "Locked",
            (_, _, "closing") => "Closing",
            (_, true, _) => "Idle",
            (_, _, "active") => "Active",
            _ => "Online",
        }
    }
}

/// One account and everything it's running right now.
#[derive(Clone, Debug, Default)]
pub struct UserUsage {
    pub uid: u32,
    pub name: String,
    /// Shares of the whole machine, like the Processes page.
    pub cpu: f32,
    /// Resident memory of every process added up. Shared pages count once per process.
    pub memory: u64,
    /// PIDs, busiest first.
    pub processes: Vec<u32>,
    pub sessions: Vec<Session>,
}

impl UserUsage {
    /// Signed in, or a person's account with something left running.
    pub fn is_person(&self) -> bool {
        !self.sessions.is_empty() || PEOPLE.contains(&self.uid)
    }

    pub fn status(&self) -> Option<&'static str> {
        // The first session is the one people are sitting at, when there is one.
        self.sessions.first().map(Session::status)
    }
}

/// Every session people log in to. The user manager logind counts as a
/// session of its own is left out, since it isn't a login.
pub fn read() -> Vec<Session> {
    let ids = list_ids();
    if ids.is_empty() {
        return Vec::new();
    }
    let mut command = Command::new("loginctl");
    command.arg("show-session").args(&ids);
    for property in PROPERTIES {
        command.args(["--property", property]);
    }
    let Ok(output) = command.output() else {
        return Vec::new();
    };
    let mut sessions = parse_sessions(&String::from_utf8_lossy(&output.stdout));
    sessions.retain(|session| !session.class.starts_with("manager"));
    sessions.sort_by(|a, b| graphical_first(a).cmp(&graphical_first(b)).then(a.started.cmp(&b.started)));
    sessions
}

/// Sorts desktop sessions ahead of terminal and remote ones.
fn graphical_first(session: &Session) -> u8 {
    match session.kind.as_str() {
        "wayland" | "x11" | "mir" => 0,
        _ => 1,
    }
}

#[derive(Deserialize)]
struct ListRow {
    session: String,
}

fn list_ids() -> Vec<String> {
    let Ok(output) = Command::new("loginctl").args(["list-sessions", "--json=short", "--no-pager"]).output() else {
        return Vec::new();
    };
    serde_json::from_slice::<Vec<ListRow>>(&output.stdout).map(|rows| rows.into_iter().map(|row| row.session).collect()).unwrap_or_default()
}

/// `show-session` prints `Key=value` lines with a blank line between sessions.
fn parse_sessions(text: &str) -> Vec<Session> {
    text.split("\n\n").filter_map(parse_session).collect()
}

fn parse_session(block: &str) -> Option<Session> {
    let fields: HashMap<&str, &str> = block.lines().filter_map(|line| line.split_once('=')).collect();
    let text = |key: &str| fields.get(key).map(|value| value.trim().to_string()).filter(|value| !value.is_empty());
    let yes = |key: &str| fields.get(key).is_some_and(|value| *value == "yes");
    Some(Session {
        id: text("Id")?,
        uid: fields.get("User")?.parse().ok()?,
        user: text("Name").unwrap_or_default(),
        class: text("Class").unwrap_or_default(),
        kind: text("Type").unwrap_or_default(),
        desktop: text("Desktop"),
        service: text("Service"),
        tty: text("TTY"),
        seat: text("Seat"),
        remote_host: text("RemoteHost"),
        state: text("State").unwrap_or_default(),
        idle: yes("IdleHint"),
        locked: yes("LockedHint"),
        started: fields.get("TimestampMonotonic").and_then(|value| value.parse().ok()).filter(|&micros| micros > 0).map(Duration::from_micros),
    })
}

/// Adds up each account's processes and pairs it with its sessions.
/// Signed-in people come first, then everyone else by CPU, then memory.
pub fn usage(processes: &[ProcessInfo], sessions: &[Session]) -> Vec<UserUsage> {
    let mut users: HashMap<u32, UserUsage> = HashMap::new();
    for process in processes.iter().filter(|process| process.state != 'X') {
        let user = users.entry(process.uid).or_insert_with(|| UserUsage { uid: process.uid, name: process.user.clone(), ..Default::default() });
        user.cpu += process.cpu;
        user.memory += process.memory;
        user.processes.push(process.pid);
    }
    for session in sessions {
        let user = users.entry(session.uid).or_insert_with(|| UserUsage { uid: session.uid, name: session.user.clone(), ..Default::default() });
        user.sessions.push(session.clone());
    }
    let by_pid: HashMap<u32, &ProcessInfo> = processes.iter().map(|process| (process.pid, process)).collect();
    let mut users: Vec<UserUsage> = users.into_values().collect();
    for user in &mut users {
        user.processes.sort_by(|a, b| by_pid[b].cpu.total_cmp(&by_pid[a].cpu).then(by_pid[b].memory.cmp(&by_pid[a].memory)));
    }
    users.sort_by(|a, b| a.sessions.is_empty().cmp(&b.sessions.is_empty()).then(b.cpu.total_cmp(&a.cpu)).then(b.memory.cmp(&a.memory)));
    users
}

/// Locks the session's screen. logind asks the desktop to lock; desktops
/// that don't listen for it won't lock.
pub fn lock(id: &str) -> Result<(), String> {
    loginctl(&["lock-session", id])
}

/// Signs the session out, ending everything running in it. logind checks
/// with polkit, which can ask for a password for other people's sessions.
pub fn terminate(id: &str) -> Result<(), String> {
    loginctl(&["terminate-session", id])
}

fn loginctl(args: &[&str]) -> Result<(), String> {
    let output = Command::new("loginctl").args(args).output().map_err(|err| format!("Couldn't run loginctl: {err}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(stderr.lines().next().unwrap_or("loginctl failed").trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::processes::Kind;

    const SHOW: &str = "Id=3\nUser=1000\nName=sizzlebop\nClass=user\nType=wayland\nDesktop=COSMIC\nService=login\nTTY=tty1\nSeat=seat0\nRemoteHost=\nState=active\nIdleHint=no\nLockedHint=yes\nTimestampMonotonic=25873534\n\nId=7\nUser=1001\nName=guest\nClass=user\nType=tty\nDesktop=\nService=sshd\nTTY=pts/0\nSeat=\nRemoteHost=10.0.0.2\nState=online\nIdleHint=yes\nLockedHint=no\nTimestampMonotonic=0\n";

    fn process(pid: u32, uid: u32, cpu: f32, memory: u64) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid: 1,
            name: format!("p{pid}"),
            command: String::new(),
            exe: None,
            user: format!("u{uid}"),
            uid,
            kind: Kind::Background,
            cgroup: String::new(),
            state: 'S',
            cpu,
            user_time: 0.,
            system_time: 0.,
            memory,
            virtual_memory: 0,
            threads: 1,
            nice: 0,
            peak_memory: None,
            shared_memory: None,
            swap: None,
            disk_read: None,
            disk_write: None,
            open_files: None,
        }
    }

    #[test]
    fn parses_show_session() {
        let sessions = parse_sessions(SHOW);
        assert_eq!(sessions.len(), 2);
        let desk = &sessions[0];
        assert_eq!((desk.id.as_str(), desk.uid, desk.user.as_str()), ("3", 1000, "sizzlebop"));
        assert_eq!(desk.label(), "Wayland · COSMIC · tty1");
        assert_eq!(desk.status(), "Locked");
        assert_eq!(desk.remote_host, None);
        assert_eq!(desk.started, Some(Duration::from_micros(25873534)));
        let ssh = &sessions[1];
        assert_eq!(ssh.label(), "Terminal · pts/0 · from 10.0.0.2");
        assert_eq!(ssh.status(), "Idle");
        assert_eq!(ssh.started, None);
    }

    #[test]
    fn adds_up_each_user() {
        let sessions = parse_sessions(SHOW);
        let mut exited = process(9, 0, 0.5, 100);
        exited.state = 'X';
        let processes = [process(1, 0, 0.01, 50), process(2, 1000, 0.02, 200), process(3, 1000, 0.10, 100), process(4, 977, 0.30, 10), exited];
        let users = usage(&processes, &sessions);
        let uids: Vec<u32> = users.iter().map(|user| user.uid).collect();
        // Signed in first, then the rest busiest first. The guest has a session but nothing running.
        assert_eq!(uids, [1000, 1001, 977, 0]);
        let me = &users[0];
        assert_eq!(me.processes, [3, 2]);
        assert_eq!(me.memory, 300);
        assert!((me.cpu - 0.12).abs() < 1e-6);
        assert_eq!(me.status(), Some("Locked"));
        assert!(me.is_person() && users[1].is_person());
        assert!(!users[2].is_person() && !users[3].is_person());
        // Exited processes don't count.
        assert_eq!(users[3].processes, [1]);
    }

    #[test]
    fn reads_this_machine() {
        // Containers and CI often have no logind; this only checks it doesn't panic.
        for session in read() {
            assert!(!session.id.is_empty());
            assert!(!session.class.starts_with("manager"));
        }
    }
}
