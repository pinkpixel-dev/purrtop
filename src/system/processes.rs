//! Every process from `/proc`, with CPU and disk use measured between readings.
//!
//! The cheap fields (from `/proc/<pid>/stat`) are read every time. The rest
//! (status, I/O, open files) only in detailed mode, while the Processes page
//! is open, since they cost several extra files per process.

use std::collections::HashMap;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use super::users::Users;

/// Whether a process belongs to an app you launched or runs in the background.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Application,
    Background,
}

impl Kind {
    /// Apps run in your user manager's `app.slice`, as `app-….scope` units
    /// started by the desktop, plain scopes, or `app-…` autostart services.
    /// Session services, system services and kernel threads are background.
    pub fn from_cgroup(cgroup: &str) -> Self {
        let Some((_, rest)) = cgroup.split_once("/app.slice/") else {
            return Kind::Background;
        };
        let unit = rest.split('/').next().unwrap_or_default();
        if unit.ends_with(".scope") || unit.starts_with("app-") { Kind::Application } else { Kind::Background }
    }
}

/// One process. `None` means the kernel didn't let us read that value
/// (another user's process) or detailed mode is off.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    /// The full command line, or `[name]` for kernel threads.
    pub command: String,
    /// The executable, from `/proc/<pid>/exe`.
    pub exe: Option<String>,
    pub user: String,
    pub uid: u32,
    pub kind: Kind,
    pub cgroup: String,
    pub state: char,
    /// Share of the whole machine's CPU since the last reading, from 0 to 1.
    pub cpu: f32,
    /// User and system CPU time so far, in seconds.
    pub user_time: f64,
    pub system_time: f64,
    /// Resident memory in bytes.
    pub memory: u64,
    pub virtual_memory: u64,
    pub threads: u32,
    pub nice: i32,
    pub peak_memory: Option<u64>,
    pub shared_memory: Option<u64>,
    pub swap: Option<u64>,
    /// Bytes per second.
    pub disk_read: Option<f64>,
    pub disk_write: Option<f64>,
    pub open_files: Option<u32>,
}

impl ProcessInfo {
    pub fn state_label(&self) -> &'static str {
        match self.state {
            'R' => "Running",
            'S' => "Sleeping",
            'D' => "Waiting",
            'Z' => "Zombie",
            'T' => "Stopped",
            't' => "Tracing",
            'I' => "Idle",
            // Dead processes never show up in /proc, so 'X' marks ones purrtop saw exit.
            'X' => "Exited",
            _ => "Unknown",
        }
    }
}

/// What `/proc/<pid>/stat` says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcStat {
    pub pid: u32,
    pub comm: String,
    pub state: char,
    pub ppid: u32,
    pub utime: u64,
    pub stime: u64,
    pub nice: i32,
    pub threads: u32,
    /// Clock ticks after boot when the process started. With the PID, it
    /// tells a process apart from a later one that reused the PID.
    pub start: u64,
    pub vsize: u64,
    pub rss_pages: u64,
}

impl ProcStat {
    /// Parses a stat line. The name sits in parentheses and may itself contain
    /// spaces or parentheses, so fields are counted from the last `)`.
    pub fn parse(text: &str) -> Option<Self> {
        let open = text.find('(')?;
        let close = text.rfind(')')?;
        let fields: Vec<&str> = text.get(close + 1..)?.split_whitespace().collect();
        let number = |ix: usize| fields.get(ix).and_then(|field| field.parse::<u64>().ok());
        let signed = |ix: usize| fields.get(ix).and_then(|field| field.parse::<i64>().ok());
        Some(Self {
            pid: text[..open].trim().parse().ok()?,
            comm: text.get(open + 1..close)?.to_string(),
            state: fields.first()?.chars().next()?,
            ppid: number(1)? as u32,
            utime: number(11)?,
            stime: number(12)?,
            nice: signed(16)? as i32,
            threads: number(17)? as u32,
            start: number(19)?,
            vsize: number(20)?,
            rss_pages: number(21)?,
        })
    }
}

/// Parts of a process that don't change while it runs, cached per PID and start time.
#[derive(Clone)]
struct Fixed {
    start: u64,
    name: String,
    command: String,
    exe: Option<String>,
    uid: u32,
    user: String,
    cgroup: String,
    kind: Kind,
}

/// Counters from the last reading, for turning totals into rates.
#[derive(Clone, Copy)]
struct Previous {
    start: u64,
    ticks: u64,
    io: Option<(u64, u64)>,
}

pub struct ProcessScanner {
    users: Users,
    fixed: HashMap<u32, Fixed>,
    previous: HashMap<u32, Previous>,
    page_size: u64,
    ticks_per_second: f64,
}

impl ProcessScanner {
    pub fn new(page_size: u64, ticks_per_second: f64) -> Self {
        Self { users: Users::load(), fixed: HashMap::new(), previous: HashMap::new(), page_size, ticks_per_second }
    }

    /// Reads every process. `machine_ticks` is how many ticks all CPUs ran
    /// since the last scan and `seconds` how long ago it was.
    pub fn scan(&mut self, detailed: bool, machine_ticks: u64, seconds: f64) -> Vec<ProcessInfo> {
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        let mut processes = Vec::new();
        let mut previous = HashMap::new();
        for entry in entries.filter_map(Result::ok) {
            let dir = entry.path();
            if !entry.file_name().to_str().is_some_and(|name| name.bytes().all(|b| b.is_ascii_digit())) {
                continue;
            }
            // Processes that exit mid-scan just drop out.
            let Some(stat) = std::fs::read_to_string(dir.join("stat")).ok().and_then(|text| ProcStat::parse(&text)) else {
                continue;
            };
            let fixed = self.fixed(&dir, &stat);
            let before = self.previous.get(&stat.pid).filter(|before| before.start == stat.start);
            let ticks = stat.utime + stat.stime;
            let cpu = match before {
                Some(before) if machine_ticks > 0 => (ticks.saturating_sub(before.ticks) as f32 / machine_ticks as f32).clamp(0., 1.),
                _ => 0.,
            };
            let io = detailed.then(|| read_io(&dir)).flatten();
            let rate = |now: u64, then: u64| if seconds > 0. { now.saturating_sub(then) as f64 / seconds } else { 0. };
            let (disk_read, disk_write) = match (io, before.and_then(|before| before.io)) {
                (Some((read, write)), Some((read_before, write_before))) => (Some(rate(read, read_before)), Some(rate(write, write_before))),
                (Some(_), None) => (Some(0.), Some(0.)),
                _ => (None, None),
            };
            let status = if detailed { read_status(&dir) } else { Status::default() };
            previous.insert(stat.pid, Previous { start: stat.start, ticks, io });

            processes.push(ProcessInfo {
                pid: stat.pid,
                ppid: stat.ppid,
                name: fixed.name,
                command: fixed.command,
                exe: fixed.exe,
                user: fixed.user,
                uid: fixed.uid,
                kind: fixed.kind,
                cgroup: fixed.cgroup,
                state: stat.state,
                cpu,
                user_time: stat.utime as f64 / self.ticks_per_second,
                system_time: stat.stime as f64 / self.ticks_per_second,
                memory: stat.rss_pages * self.page_size,
                virtual_memory: stat.vsize,
                threads: stat.threads,
                nice: stat.nice,
                peak_memory: status.peak,
                shared_memory: status.shared,
                swap: status.swap,
                disk_read,
                disk_write,
                open_files: detailed.then(|| count_open_files(&dir)).flatten(),
            });
        }
        // Rebuilt each scan, so exited processes leave the caches.
        self.previous = previous;
        self.fixed.retain(|pid, _| self.previous.contains_key(pid));
        processes
    }

    fn fixed(&mut self, dir: &Path, stat: &ProcStat) -> Fixed {
        if let Some(fixed) = self.fixed.get(&stat.pid).filter(|fixed| fixed.start == stat.start) {
            return fixed.clone();
        }
        let args = read_cmdline(dir);
        let uid = std::fs::metadata(dir).map(|meta| meta.uid()).unwrap_or(0);
        let cgroup = std::fs::read_to_string(dir.join("cgroup"))
            .ok()
            .and_then(|text| text.lines().find_map(|line| line.strip_prefix("0::")).map(str::to_string))
            .unwrap_or_default();
        let fixed = Fixed {
            start: stat.start,
            name: display_name(&stat.comm, args.first().map(String::as_str)),
            command: if args.is_empty() { format!("[{}]", stat.comm) } else { args.join(" ") },
            exe: std::fs::read_link(dir.join("exe")).ok().map(|path| path.to_string_lossy().into_owned()),
            uid,
            user: self.users.name(uid),
            kind: Kind::from_cgroup(&cgroup),
            cgroup,
        };
        self.fixed.insert(stat.pid, fixed.clone());
        fixed
    }
}

/// `comm` is cut at 15 characters. When it was cut and the first argument's
/// file name starts with it, the file name is the real name.
fn display_name(comm: &str, argv0: Option<&str>) -> String {
    const COMM_LIMIT: usize = 15;
    if comm.len() >= COMM_LIMIT
        && let Some(base) = argv0.and_then(|arg| arg.rsplit('/').next())
        && base.starts_with(comm)
    {
        return base.to_string();
    }
    comm.to_string()
}

fn read_cmdline(dir: &Path) -> Vec<String> {
    let bytes = std::fs::read(dir.join("cmdline")).unwrap_or_default();
    bytes
        .split(|&b| b == 0)
        .filter(|arg| !arg.is_empty())
        .map(|arg| String::from_utf8_lossy(arg).into_owned())
        .collect()
}

/// Bytes read from and written to storage, from `/proc/<pid>/io`. Only
/// readable for your own processes.
fn read_io(dir: &Path) -> Option<(u64, u64)> {
    parse_io(&std::fs::read_to_string(dir.join("io")).ok()?)
}

fn parse_io(text: &str) -> Option<(u64, u64)> {
    let field = |key: &str| text.lines().find_map(|line| line.strip_prefix(key)).and_then(|value| value.trim().parse().ok());
    Some((field("read_bytes:")?, field("write_bytes:")?))
}

#[derive(Default)]
struct Status {
    peak: Option<u64>,
    shared: Option<u64>,
    swap: Option<u64>,
}

fn read_status(dir: &Path) -> Status {
    parse_status(&std::fs::read_to_string(dir.join("status")).unwrap_or_default())
}

/// Peak resident memory, shared memory (file-backed plus shmem) and swap, in bytes.
fn parse_status(text: &str) -> Status {
    let kb = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .and_then(|value| value.split_whitespace().next())
            .and_then(|value| value.parse::<u64>().ok())
            .map(|kb| kb * 1024)
    };
    let shared = match (kb("RssFile:"), kb("RssShmem:")) {
        (None, None) => None,
        (file, shmem) => Some(file.unwrap_or(0) + shmem.unwrap_or(0)),
    };
    Status { peak: kb("VmHWM:"), shared, swap: kb("VmSwap:") }
}

fn count_open_files(dir: &Path) -> Option<u32> {
    Some(std::fs::read_dir(dir.join("fd")).ok()?.count() as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &str = "1234 (Web Content (x)) S 1 1234 1234 0 -1 4194560 100 0 0 0 150 50 0 0 20 -5 30 0 5000 999999 2048 18446744073709551615";

    #[test]
    fn parses_stat_lines() {
        let stat = ProcStat::parse(STAT).unwrap();
        assert_eq!(stat.pid, 1234);
        assert_eq!(stat.comm, "Web Content (x)");
        assert_eq!((stat.state, stat.ppid), ('S', 1));
        assert_eq!((stat.utime, stat.stime), (150, 50));
        assert_eq!((stat.nice, stat.threads), (-5, 30));
        assert_eq!((stat.start, stat.vsize, stat.rss_pages), (5000, 999999, 2048));
        assert_eq!(ProcStat::parse("garbage"), None);
    }

    #[test]
    fn sorts_apps_from_background() {
        let app = "/user.slice/user-1000.slice/user@1000.service/app.slice/app-com.google.Chrome-2152465.scope";
        let scope = "/user.slice/user-1000.slice/user@1000.service/app.slice/cosmic-bg.scope";
        let autostart = "/user.slice/user-1000.slice/user@1000.service/app.slice/app-kitty@autostart.service";
        let portal = "/user.slice/user-1000.slice/user@1000.service/app.slice/xdg-desktop-portal.service";
        let session = "/user.slice/user-1000.slice/user@1000.service/session.slice/pipewire.service";
        assert_eq!(Kind::from_cgroup(app), Kind::Application);
        assert_eq!(Kind::from_cgroup(scope), Kind::Application);
        assert_eq!(Kind::from_cgroup(autostart), Kind::Application);
        assert_eq!(Kind::from_cgroup(portal), Kind::Background);
        assert_eq!(Kind::from_cgroup(session), Kind::Background);
        assert_eq!(Kind::from_cgroup("/init.scope"), Kind::Background);
        assert_eq!(Kind::from_cgroup(""), Kind::Background);
    }

    #[test]
    fn uses_the_full_name_when_comm_was_cut() {
        assert_eq!(display_name("xdg-desktop-por", Some("/usr/lib/xdg-desktop-portal-cosmic")), "xdg-desktop-portal-cosmic");
        assert_eq!(display_name("bash", Some("/usr/bin/bash")), "bash");
        // Renamed processes keep the name they gave themselves.
        assert_eq!(display_name("Web Content", Some("/usr/lib/firefox/firefox")), "Web Content");
        assert_eq!(display_name("kworker/u128:1-", None), "kworker/u128:1-");
    }

    #[test]
    fn parses_io_and_status() {
        assert_eq!(parse_io("rchar: 9\nread_bytes: 4096\nwrite_bytes: 512\ncancelled_write_bytes: 0\n"), Some((4096, 512)));
        assert_eq!(parse_io(""), None);
        let status = parse_status("VmHWM:\t  47100 kB\nRssFile:\t  10000 kB\nRssShmem:\t   4600 kB\nVmSwap:\t  14000 kB\n");
        assert_eq!(status.peak, Some(47_100 * 1024));
        assert_eq!(status.shared, Some(14_600 * 1024));
        assert_eq!(status.swap, Some(14_000 * 1024));
        assert_eq!(parse_status("Name: kthreadd\n").shared, None);
    }

    #[test]
    fn scans_this_machine() {
        let mut scanner = ProcessScanner::new(4096, 100.);
        let processes = scanner.scan(true, 0, 0.);
        let me = processes.iter().find(|process| process.pid == std::process::id()).expect("the test process is listed");
        assert!(me.memory > 0);
        assert!(me.open_files.is_some(), "our own open files are readable");
        assert!(processes.iter().any(|process| process.pid == 1));
    }
}
