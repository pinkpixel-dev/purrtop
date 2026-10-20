//! Acting on processes: signals, nice, CPU affinity, stack samples.
//!
//! Each action is tried directly first. When the kernel refuses (another
//! user's process, or a nice below 0), the caller can retry the same action
//! through `pkexec`, which asks for your password with the desktop's polkit agent.

use std::path::PathBuf;
use std::process::Command;

use super::processes::ProcessInfo;

/// The kernel said no; the action needs administrator rights.
#[derive(Debug, PartialEq, Eq)]
pub struct Denied;

/// Common signals for the Send signal menu: number and name.
pub const SIGNALS: [(i32, &str); 10] = [
    (libc::SIGHUP, "SIGHUP (hang up)"),
    (libc::SIGINT, "SIGINT (interrupt)"),
    (libc::SIGQUIT, "SIGQUIT (quit)"),
    (libc::SIGTERM, "SIGTERM (terminate)"),
    (libc::SIGKILL, "SIGKILL (kill)"),
    (libc::SIGUSR1, "SIGUSR1"),
    (libc::SIGUSR2, "SIGUSR2"),
    (libc::SIGSTOP, "SIGSTOP (suspend)"),
    (libc::SIGCONT, "SIGCONT (resume)"),
    (libc::SIGWINCH, "SIGWINCH (window resized)"),
];

/// Sends `signal` to every PID in order. Processes that already exited are
/// skipped. Stops at the first one the kernel refuses.
pub fn signal(pids: &[u32], signal: i32) -> Result<(), Denied> {
    for &pid in pids {
        // SAFETY: kill only takes plain integers.
        if unsafe { libc::kill(pid as libc::pid_t, signal) } != 0 {
            match std::io::Error::last_os_error().raw_os_error() {
                Some(libc::ESRCH) => continue,
                _ => return Err(Denied),
            }
        }
    }
    Ok(())
}

/// `root` and everything it started, children before parents, so a tree
/// is ended from the leaves up.
pub fn tree(processes: &[ProcessInfo], root: u32) -> Vec<u32> {
    let mut order = Vec::new();
    collect(processes, root, &mut order, 0);
    order
}

fn collect(processes: &[ProcessInfo], pid: u32, order: &mut Vec<u32>, depth: usize) {
    // Guards against a malformed parent chain looping forever.
    if depth > 256 || order.contains(&pid) {
        return;
    }
    for child in processes.iter().filter(|process| process.ppid == pid && process.pid != pid) {
        collect(processes, child.pid, order, depth + 1);
    }
    order.push(pid);
}

/// Thread IDs of a process. Nice and affinity are per thread on Linux.
fn threads(pid: u32) -> Vec<u32> {
    let mut tids: Vec<u32> = std::fs::read_dir(format!("/proc/{pid}/task"))
        .map(|entries| entries.filter_map(Result::ok).filter_map(|entry| entry.file_name().to_str()?.parse().ok()).collect())
        .unwrap_or_default();
    if tids.is_empty() {
        tids.push(pid);
    }
    tids
}

/// Sets the nice value of every thread in the process.
pub fn renice(pid: u32, nice: i32) -> Result<(), Denied> {
    for tid in threads(pid) {
        // SAFETY: setpriority only takes plain integers.
        if unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, nice) } != 0 {
            return Err(Denied);
        }
    }
    Ok(())
}

/// The CPUs the process may run on, by index.
pub fn affinity(pid: u32) -> Option<Vec<usize>> {
    // SAFETY: cpu_set_t is plain data, zeroed is a valid empty set, and the
    // kernel writes at most size_of::<cpu_set_t>() bytes.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        if libc::sched_getaffinity(pid as libc::pid_t, std::mem::size_of::<libc::cpu_set_t>(), &mut set) != 0 {
            return None;
        }
        Some((0..libc::CPU_SETSIZE as usize).filter(|&cpu| libc::CPU_ISSET(cpu, &set)).collect())
    }
}

/// Limits every thread in the process to `cpus`.
pub fn set_affinity(pid: u32, cpus: &[usize]) -> Result<(), Denied> {
    // SAFETY: as in `affinity`; CPU_SET only touches bits inside the set.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        for &cpu in cpus {
            libc::CPU_SET(cpu, &mut set);
        }
        for tid in threads(pid) {
            if libc::sched_setaffinity(tid as libc::pid_t, std::mem::size_of::<libc::cpu_set_t>(), &set) != 0 {
                return Err(Denied);
            }
        }
    }
    Ok(())
}

/// The same actions as root, for `pkexec`.
pub enum AdminAction {
    Signal { pids: Vec<u32>, signal: i32 },
    Renice { pid: u32, nice: i32 },
    Affinity { pid: u32, cpus: Vec<usize> },
}

impl AdminAction {
    fn command(&self) -> Vec<String> {
        match self {
            AdminAction::Signal { pids, signal } => {
                let mut args = vec!["/usr/bin/kill".into(), "--signal".into(), signal.to_string()];
                args.extend(pids.iter().map(u32::to_string));
                args
            }
            AdminAction::Renice { pid, nice } => {
                let mut args = vec!["/usr/bin/renice".into(), "-n".into(), nice.to_string(), "-p".into()];
                args.extend(threads(*pid).iter().map(u32::to_string));
                args
            }
            AdminAction::Affinity { pid, cpus } => vec![
                "/usr/bin/taskset".into(),
                "--all-tasks".into(),
                "--cpu-list".into(),
                "--pid".into(),
                cpus.iter().map(usize::to_string).collect::<Vec<_>>().join(","),
                pid.to_string(),
            ],
        }
    }

    /// Runs the action through pkexec. Blocks until you answer the password
    /// prompt, so call it off the UI thread.
    pub fn run(&self) -> Result<(), String> {
        run_pkexec(&self.command()).map(|_| ())
    }
}

/// Runs `command` as root through pkexec and returns what it printed. Blocks.
pub fn run_pkexec(command: &[String]) -> Result<String, String> {
    let output = Command::new("pkexec").args(command).output().map_err(|err| format!("Couldn't start pkexec: {err}"))?;
    match output.status.code() {
        Some(0) => Ok(String::from_utf8_lossy(&output.stdout).into_owned()),
        // pkexec's own codes for a dismissed or failed password prompt.
        Some(126) | Some(127) if output.stdout.is_empty() => Err("Authentication was cancelled or failed".into()),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(stderr.lines().last().unwrap_or("The command failed").trim().to_string())
        }
    }
}

/// Captures every thread's stack with `eu-stack` and saves it to a text file
/// in purrtop's cache folder. Runs through pkexec, since reading another
/// process's stacks needs ptrace rights. Blocks; call it off the UI thread.
pub fn sample(pid: u32, name: &str) -> Result<PathBuf, String> {
    let stacks = run_pkexec(&["/usr/bin/eu-stack".into(), "--pid".into(), pid.to_string()])?;
    let dir = cache_dir().ok_or("No cache folder (HOME isn't set)")?.join("samples");
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|time| time.as_secs()).unwrap_or(0);
    let safe_name: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    let path = dir.join(format!("{safe_name}-{pid}-{stamp}.txt"));
    std::fs::write(&path, format!("Stack sample of {name} (PID {pid})\n\n{stacks}")).map_err(|err| err.to_string())?;
    Ok(path)
}

fn cache_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|dir| dir.join("purrtop"))
}

/// Shows the file in the file manager, selected when the file manager
/// supports it, otherwise opens the folder it's in.
pub fn show_in_file_manager(path: &str) -> Result<(), String> {
    let uri = format!("file://{}", percent_encode(path, true));
    let selected = Command::new("gdbus")
        .args([
            "call",
            "--session",
            "--dest",
            "org.freedesktop.FileManager1",
            "--object-path",
            "/org/freedesktop/FileManager1",
            "--method",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("['{uri}']"),
            "",
        ])
        .output()
        .is_ok_and(|output| output.status.success());
    if selected {
        return Ok(());
    }
    let folder = std::path::Path::new(path).parent().ok_or("The file has no folder")?;
    Command::new("xdg-open").arg(folder).spawn().map(|_| ()).map_err(|err| format!("Couldn't open the folder: {err}"))
}

/// A web search for what a process is.
pub fn search_url(name: &str) -> String {
    format!("https://duckduckgo.com/?q={}", percent_encode(&format!("{name} linux process"), false))
}

/// Percent-encodes everything but unreserved characters, and `/` when `keep_slash`.
fn percent_encode(text: &str, keep_slash: bool) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => encoded.push(byte as char),
            b'/' if keep_slash => encoded.push('/'),
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::processes::Kind;

    fn process(pid: u32, ppid: u32) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid,
            name: format!("p{pid}"),
            command: String::new(),
            exe: None,
            user: String::new(),
            uid: 0,
            kind: Kind::Background,
            cgroup: String::new(),
            state: 'S',
            cpu: 0.,
            user_time: 0.,
            system_time: 0.,
            memory: 0,
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
    fn trees_end_leaves_first() {
        let processes = [process(1, 0), process(10, 1), process(11, 10), process(12, 10), process(20, 1)];
        let order = tree(&processes, 10);
        assert_eq!(order.len(), 3);
        assert_eq!(order.last(), Some(&10));
        assert!(order.contains(&11) && order.contains(&12));
        assert_eq!(tree(&processes, 20), vec![20]);
    }

    #[test]
    fn builds_admin_commands() {
        let signal = AdminAction::Signal { pids: vec![5, 6], signal: 9 }.command();
        assert_eq!(signal, ["/usr/bin/kill", "--signal", "9", "5", "6"]);
        let affinity = AdminAction::Affinity { pid: 7, cpus: vec![0, 2, 3] }.command();
        assert_eq!(affinity[4..], ["0,2,3".to_string(), "7".to_string()]);
    }

    #[test]
    fn encodes_urls() {
        assert_eq!(search_url("cosmic-bg"), "https://duckduckgo.com/?q=cosmic-bg%20linux%20process");
        assert_eq!(percent_encode("/opt/My App/app", true), "/opt/My%20App/app");
    }

    #[test]
    fn signals_and_affinity_work_on_our_own_process() {
        let me = std::process::id();
        // Signal 0 only checks that we're allowed to signal.
        assert_eq!(signal(&[me], 0), Ok(()));
        // PID 1 belongs to root.
        assert_eq!(signal(&[1], 0), Err(Denied));
        let cpus = affinity(me).expect("our own affinity is readable");
        assert!(!cpus.is_empty());
    }
}
