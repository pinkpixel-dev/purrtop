//! What's gone wrong lately: failed systemd units, error messages this boot,
//! recent crashes and OOM kills. Read through `systemctl`, `journalctl` and
//! `coredumpctl` when the System Info Diagnostics tab asks, never on a timer.

use std::collections::HashMap;
use std::process::Command;

use serde::Deserialize;
use serde_json::Value;

/// How far back the crash list goes, in `coredumpctl --since` form.
const CRASH_WINDOW: &str = "-7d";
/// Longer messages are cut here. The journal is a click away for the rest.
const MESSAGE_LIMIT: usize = 300;
/// The kernel's OOM killer and systemd-oomd both say this when they kill something.
const OOM_PATTERN: &str = "Killed process|due to memory|due to swap";

/// Each section is read on its own, so one missing tool only costs its section.
#[derive(Clone, Debug)]
pub struct Diagnostics {
    pub failed_units: Result<Vec<FailedUnit>, String>,
    pub errors: Result<ErrorSummary, String>,
    pub crashes: Result<Vec<Crash>, String>,
    pub oom_kills: Result<Vec<OomKill>, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailedUnit {
    pub name: String,
    pub description: String,
    /// A unit of your own user manager rather than the system's.
    pub user: bool,
}

/// Error-or-worse journal entries this boot, grouped by source and message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ErrorSummary {
    /// Most recent first.
    pub groups: Vec<ErrorGroup>,
    pub total: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErrorGroup {
    pub source: String,
    /// The first line of the message.
    pub message: String,
    pub count: usize,
    /// When it last happened, in microseconds since the Unix epoch.
    pub last_us: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Crash {
    pub time_us: u64,
    pub pid: u32,
    pub exe: String,
    pub signal: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OomKill {
    pub time_us: u64,
    /// The process or unit that was killed.
    pub target: String,
    /// `Kernel` or `systemd-oomd`.
    pub by: &'static str,
}

impl Diagnostics {
    pub fn read() -> Self {
        Self {
            failed_units: read_failed_units(),
            errors: run("journalctl", &["-b", "-p", "err", "-q", "--no-pager", "-o", "json", "--output-fields=MESSAGE,SYSLOG_IDENTIFIER,_COMM"])
                .map(|text| group_errors(&text)),
            crashes: run("coredumpctl", &["list", "--no-pager", "--json=short", &format!("--since={CRASH_WINDOW}")])
                .and_then(|text| parse_crashes(&text)),
            oom_kills: run("journalctl", &["-b", "-q", "--no-pager", "-o", "json", "--output-fields=MESSAGE,SYSLOG_IDENTIFIER", "-g", OOM_PATTERN])
                .map(|text| parse_oom_kills(&text)),
        }
    }
}

/// Runs a tool and returns what it printed. A non-zero exit with nothing on
/// stdout and no error text counts as "nothing found", which is how
/// `coredumpctl` and `journalctl -g` report an empty result.
fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let output = Command::new(program).args(args).output().map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => format!("{program} isn't installed"),
        _ => format!("Couldn't run {program}: {err}"),
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    if output.status.success() || (stdout.trim().is_empty() && is_empty_result(&String::from_utf8_lossy(&output.stderr))) {
        return Ok(stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(stderr.lines().next().filter(|line| !line.is_empty()).map_or_else(|| format!("{program} failed"), str::to_string))
}

fn is_empty_result(stderr: &str) -> bool {
    let stderr = stderr.trim();
    stderr.is_empty() || stderr.starts_with("No coredumps found") || stderr.starts_with("-- No entries --")
}

#[derive(Deserialize)]
struct UnitRow {
    unit: String,
    #[serde(default)]
    description: String,
}

fn read_failed_units() -> Result<Vec<FailedUnit>, String> {
    let list = |user: bool| -> Result<Vec<FailedUnit>, String> {
        let mut args = vec!["--failed", "--output=json", "--no-pager"];
        if user {
            args.insert(0, "--user");
        }
        let text = run("systemctl", &args)?;
        Ok(parse_units(&text)?.into_iter().map(|(name, description)| FailedUnit { name, description, user }).collect())
    };
    let mut units = list(false)?;
    // No user manager (a root session, say) just means no user units.
    units.extend(list(true).unwrap_or_default());
    Ok(units)
}

fn parse_units(text: &str) -> Result<Vec<(String, String)>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<UnitRow> = serde_json::from_str(text).map_err(|err| format!("Couldn't read systemctl's output: {err}"))?;
    Ok(rows.into_iter().map(|row| (unescape_unit(&row.unit), row.description)).collect())
}

/// systemd escapes `-` in unit names as `\x2d`. Show the plain name.
fn unescape_unit(name: &str) -> String {
    name.replace("\\x2d", "-")
}

/// One JSON object per line. Bad lines are skipped.
fn journal_entries(text: &str) -> impl Iterator<Item = HashMap<String, Value>> + '_ {
    text.lines().filter_map(|line| serde_json::from_str(line).ok())
}

/// A journal field as text. Binary fields come as arrays of bytes.
fn field(entry: &HashMap<String, Value>, key: &str) -> Option<String> {
    match entry.get(key)? {
        Value::String(text) => Some(text.clone()),
        Value::Array(bytes) => {
            let bytes: Vec<u8> = bytes.iter().filter_map(|byte| byte.as_u64().map(|byte| byte as u8)).collect();
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
        _ => None,
    }
}

fn timestamp(entry: &HashMap<String, Value>) -> u64 {
    field(entry, "__REALTIME_TIMESTAMP").and_then(|time| time.parse().ok()).unwrap_or(0)
}

fn first_line(message: &str) -> String {
    let line = message.lines().find(|line| !line.trim().is_empty()).unwrap_or("").trim();
    match line.char_indices().nth(MESSAGE_LIMIT) {
        Some((cut, _)) => format!("{}…", &line[..cut]),
        None => line.to_string(),
    }
}

fn group_errors(text: &str) -> ErrorSummary {
    let mut groups: HashMap<(String, String), ErrorGroup> = HashMap::new();
    let mut total = 0;
    for entry in journal_entries(text) {
        let Some(message) = field(&entry, "MESSAGE") else {
            continue;
        };
        total += 1;
        let source = field(&entry, "SYSLOG_IDENTIFIER").or_else(|| field(&entry, "_COMM")).unwrap_or_else(|| "unknown".into());
        let message = first_line(&message);
        let time = timestamp(&entry);
        let group = groups
            .entry((source.clone(), message.clone()))
            .or_insert(ErrorGroup { source, message, count: 0, last_us: 0 });
        group.count += 1;
        group.last_us = group.last_us.max(time);
    }
    let mut groups: Vec<ErrorGroup> = groups.into_values().collect();
    groups.sort_by(|a, b| b.last_us.cmp(&a.last_us).then_with(|| a.source.cmp(&b.source)));
    ErrorSummary { groups, total }
}

#[derive(Deserialize)]
struct CrashRow {
    time: u64,
    pid: u32,
    #[serde(default)]
    exe: Option<String>,
    #[serde(default)]
    sig: Option<i32>,
}

fn parse_crashes(text: &str) -> Result<Vec<Crash>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<CrashRow> = serde_json::from_str(text).map_err(|err| format!("Couldn't read coredumpctl's output: {err}"))?;
    let mut crashes: Vec<Crash> = rows
        .into_iter()
        .map(|row| Crash { time_us: row.time, pid: row.pid, exe: row.exe.unwrap_or_else(|| "unknown".into()), signal: row.sig })
        .collect();
    crashes.sort_by_key(|crash| std::cmp::Reverse(crash.time_us));
    Ok(crashes)
}

fn parse_oom_kills(text: &str) -> Vec<OomKill> {
    let mut kills: Vec<OomKill> = journal_entries(text)
        .filter_map(|entry| {
            let message = field(&entry, "MESSAGE")?;
            let (target, by) = match field(&entry, "SYSLOG_IDENTIFIER").as_deref() {
                Some("kernel") => (kernel_oom_target(&message)?, "Kernel"),
                Some("systemd-oomd") => (oomd_target(&message)?, "systemd-oomd"),
                _ => return None,
            };
            Some(OomKill { time_us: timestamp(&entry), target, by })
        })
        .collect();
    kills.sort_by_key(|kill| std::cmp::Reverse(kill.time_us));
    kills
}

/// `Out of memory: Killed process 1234 (firefox) total-vm:…` gives `firefox`.
fn kernel_oom_target(message: &str) -> Option<String> {
    let (_, rest) = message.split_once("Killed process ")?;
    let (_, rest) = rest.split_once('(')?;
    let (name, _) = rest.split_once(')')?;
    Some(name.to_string())
}

/// `Killed /user.slice/…/app-firefox-1234.scope due to memory pressure …`
/// gives the unit, `app-firefox-1234.scope`.
fn oomd_target(message: &str) -> Option<String> {
    let rest = message.strip_prefix("Killed ")?;
    let (path, _) = rest.split_once(" due to ")?;
    path.rsplit('/').next().filter(|unit| !unit.is_empty()).map(unescape_unit)
}

/// The usual crash signals by name, or the number for anything else.
pub fn signal_name(signal: i32) -> String {
    match signal {
        libc::SIGQUIT => "SIGQUIT".into(),
        libc::SIGILL => "SIGILL".into(),
        libc::SIGTRAP => "SIGTRAP".into(),
        libc::SIGABRT => "SIGABRT".into(),
        libc::SIGBUS => "SIGBUS".into(),
        libc::SIGFPE => "SIGFPE".into(),
        libc::SIGSEGV => "SIGSEGV".into(),
        libc::SIGSYS => "SIGSYS".into(),
        other => format!("Signal {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_units_from_json() {
        let text = r#"[{"unit":"app-synology\\x2ddrive\\x2dopen\\x2dfile@autostart.service","load":"loaded","active":"failed","sub":"failed","description":"Synology Drive"}]"#;
        assert_eq!(
            parse_units(text).unwrap(),
            vec![("app-synology-drive-open-file@autostart.service".into(), "Synology Drive".into())]
        );
        assert_eq!(parse_units("[]\n").unwrap(), vec![]);
        assert_eq!(parse_units("").unwrap(), vec![]);
        assert!(parse_units("not json").is_err());
    }

    #[test]
    fn errors_group_by_source_and_message() {
        let text = concat!(
            r#"{"__REALTIME_TIMESTAMP":"100","SYSLOG_IDENTIFIER":"cosmic-comp","MESSAGE":"surface missing"}"#,
            "\n",
            r#"{"__REALTIME_TIMESTAMP":"300","SYSLOG_IDENTIFIER":"cosmic-comp","MESSAGE":"surface missing"}"#,
            "\n",
            r#"{"__REALTIME_TIMESTAMP":"200","_COMM":"foo","MESSAGE":[104,105,10,116,104,101,114,101]}"#,
            "\n",
            "garbage\n",
        );
        let summary = group_errors(text);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.groups.len(), 2);
        assert_eq!(summary.groups[0], ErrorGroup { source: "cosmic-comp".into(), message: "surface missing".into(), count: 2, last_us: 300 });
        // Binary message, first line only, source from _COMM.
        assert_eq!(summary.groups[1].source, "foo");
        assert_eq!(summary.groups[1].message, "hi");
    }

    #[test]
    fn long_messages_are_cut() {
        let long = "x".repeat(MESSAGE_LIMIT + 50);
        assert_eq!(first_line(&long).chars().count(), MESSAGE_LIMIT + 1);
        assert_eq!(first_line("\n  first  \nsecond"), "first");
    }

    #[test]
    fn crashes_newest_first() {
        let text = r#"[{"time":1,"pid":10,"uid":1000,"gid":1000,"sig":6,"corefile":"present","exe":"/usr/bin/a","size":1},{"time":5,"pid":11,"uid":1000,"gid":1000,"sig":11,"corefile":"missing","exe":"/usr/bin/b","size":null}]"#;
        let crashes = parse_crashes(text).unwrap();
        assert_eq!(crashes[0], Crash { time_us: 5, pid: 11, exe: "/usr/bin/b".into(), signal: Some(11) });
        assert_eq!(crashes.len(), 2);
        assert_eq!(parse_crashes("").unwrap(), vec![]);
    }

    #[test]
    fn oom_kills_from_kernel_and_oomd() {
        let text = concat!(
            r#"{"__REALTIME_TIMESTAMP":"10","SYSLOG_IDENTIFIER":"kernel","MESSAGE":"Out of memory: Killed process 4242 (firefox) total-vm:123kB, anon-rss:4kB"}"#,
            "\n",
            r#"{"__REALTIME_TIMESTAMP":"20","SYSLOG_IDENTIFIER":"systemd-oomd","MESSAGE":"Killed /user.slice/user-1000.slice/user@1000.service/app.slice/app-makepkg\\x2dbuild.scope due to memory pressure for /user.slice being 75.43% > 50.00% for > 20s with reclaim activity"}"#,
            "\n",
            r#"{"__REALTIME_TIMESTAMP":"30","SYSLOG_IDENTIFIER":"kernel","MESSAGE":"NVRM: Check failed: Out of memory [NV_ERR_NO_MEMORY]"}"#,
            "\n",
            r#"{"__REALTIME_TIMESTAMP":"40","SYSLOG_IDENTIFIER":"bash","MESSAGE":"Killed process 1 (x) due to memory"}"#,
            "\n",
        );
        let kills = parse_oom_kills(text);
        assert_eq!(
            kills,
            vec![
                OomKill { time_us: 20, target: "app-makepkg-build.scope".into(), by: "systemd-oomd" },
                OomKill { time_us: 10, target: "firefox".into(), by: "Kernel" },
            ]
        );
    }

    #[test]
    fn signal_names() {
        assert_eq!(signal_name(6), "SIGABRT");
        assert_eq!(signal_name(11), "SIGSEGV");
        assert_eq!(signal_name(64), "Signal 64");
    }

    #[test]
    fn empty_results_are_not_errors() {
        assert!(is_empty_result("No coredumps found.\n"));
        assert!(is_empty_result(""));
        assert!(!is_empty_result("Failed to connect to bus: No medium found"));
    }

    #[test]
    fn reads_this_machine() {
        let diagnostics = Diagnostics::read();
        assert!(diagnostics.failed_units.is_ok(), "{:?}", diagnostics.failed_units);
        assert!(diagnostics.errors.is_ok(), "{:?}", diagnostics.errors);
        assert!(diagnostics.crashes.is_ok(), "{:?}", diagnostics.crashes);
        assert!(diagnostics.oom_kills.is_ok(), "{:?}", diagnostics.oom_kills);
    }
}
