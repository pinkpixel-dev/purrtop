//! purrtop's admin helper, run as root through `pkexec` so polkit can keep
//! the password for a few minutes (the `dev.pinkpixel.purrtop.run-helper`
//! action). It only does the five things purrtop needs, and checks every
//! argument before doing anything:
//!
//! ```text
//! purrtop-helper signal <signal> <pid>...   send a signal from purrtop's menu
//! purrtop-helper renice <nice> <pid>        set every thread's nice value
//! purrtop-helper affinity <cpu,...> <pid>   limit every thread to these CPUs
//! purrtop-helper sample <pid>               print every thread's stack
//! purrtop-helper port-owners                print listening sockets with owners
//! ```
//!
//! Errors go to stderr with exit code 1. 126 and 127 are left to pkexec,
//! which uses them for a cancelled or failed password prompt.

use std::process::{Command, ExitCode};

/// The signals purrtop's menus send: hang up, interrupt, quit, terminate,
/// kill, the two user signals, stop, continue, and window resized.
const SIGNALS: [i32; 10] = [libc::SIGHUP, libc::SIGINT, libc::SIGQUIT, libc::SIGTERM, libc::SIGKILL, libc::SIGUSR1, libc::SIGUSR2, libc::SIGSTOP, libc::SIGCONT, libc::SIGWINCH];
/// More than any process tree purrtop would end at once.
const MAX_PIDS: usize = 4096;

#[derive(Debug, PartialEq, Eq)]
enum Request {
    Signal { signal: i32, pids: Vec<u32> },
    Renice { nice: i32, pid: u32 },
    Affinity { cpus: Vec<usize>, pid: u32 },
    Sample { pid: u32 },
    PortOwners,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args).and_then(run) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("purrtop-helper: {err}");
            ExitCode::from(1)
        }
    }
}

fn parse(args: &[String]) -> Result<Request, String> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["signal", signal, pids @ ..] => {
            let signal: i32 = signal.parse().map_err(|_| format!("not a signal number: {signal}"))?;
            if !SIGNALS.contains(&signal) {
                return Err(format!("signal {signal} isn't one purrtop sends"));
            }
            if pids.is_empty() || pids.len() > MAX_PIDS {
                return Err(format!("expected 1 to {MAX_PIDS} PIDs"));
            }
            Ok(Request::Signal { signal, pids: pids.iter().map(|pid| parse_pid(pid)).collect::<Result<_, _>>()? })
        }
        ["renice", nice, pid] => {
            let nice: i32 = nice.parse().map_err(|_| format!("not a nice value: {nice}"))?;
            if !(-20..=19).contains(&nice) {
                return Err(format!("nice {nice} is outside -20 to 19"));
            }
            Ok(Request::Renice { nice, pid: parse_pid(pid)? })
        }
        ["affinity", cpus, pid] => {
            let cpus: Vec<usize> = cpus.split(',').map(|cpu| cpu.parse().ok().filter(|&cpu| cpu < libc::CPU_SETSIZE as usize)).collect::<Option<_>>().ok_or(format!("not a CPU list: {cpus}"))?;
            if cpus.is_empty() {
                return Err("no CPUs given".into());
            }
            Ok(Request::Affinity { cpus, pid: parse_pid(pid)? })
        }
        ["sample", pid] => Ok(Request::Sample { pid: parse_pid(pid)? }),
        ["port-owners"] => Ok(Request::PortOwners),
        _ => Err("unknown request".into()),
    }
}

/// A plain decimal PID above 1. As root, 0 would mean a whole process group,
/// and 1 is systemd itself.
fn parse_pid(text: &str) -> Result<u32, String> {
    match text.parse::<u32>() {
        Ok(pid) if pid > 1 && text.bytes().all(|b| b.is_ascii_digit()) => Ok(pid),
        _ => Err(format!("not a PID purrtop acts on: {text}")),
    }
}

fn run(request: Request) -> Result<(), String> {
    match request {
        Request::Signal { signal, pids } => {
            for pid in pids {
                // SAFETY: kill only takes plain integers.
                if unsafe { libc::kill(pid as libc::pid_t, signal) } != 0 {
                    let err = std::io::Error::last_os_error();
                    // It already exited, which is what ending it wanted anyway.
                    if err.raw_os_error() != Some(libc::ESRCH) {
                        return Err(format!("couldn't signal {pid}: {err}"));
                    }
                }
            }
            Ok(())
        }
        Request::Renice { nice, pid } => {
            for tid in threads(pid)? {
                // SAFETY: setpriority only takes plain integers.
                if unsafe { libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, nice) } != 0 {
                    return Err(format!("couldn't set the nice value of {tid}: {}", std::io::Error::last_os_error()));
                }
            }
            Ok(())
        }
        Request::Affinity { cpus, pid } => {
            // SAFETY: cpu_set_t is plain data, zeroed is a valid empty set,
            // CPU_SET only touches bits inside it, and the CPUs were checked
            // to be below CPU_SETSIZE.
            unsafe {
                let mut set: libc::cpu_set_t = std::mem::zeroed();
                for &cpu in &cpus {
                    libc::CPU_SET(cpu, &mut set);
                }
                for tid in threads(pid)? {
                    if libc::sched_setaffinity(tid as libc::pid_t, std::mem::size_of::<libc::cpu_set_t>(), &set) != 0 {
                        return Err(format!("couldn't set the CPUs of {tid}: {}", std::io::Error::last_os_error()));
                    }
                }
            }
            Ok(())
        }
        Request::Sample { pid } => pass_through("/usr/bin/eu-stack", &["--pid".into(), pid.to_string()]),
        Request::PortOwners => {
            let args = ["--tcp", "--udp", "--listening", "--numeric", "--processes", "--extended", "--no-header"];
            pass_through("/usr/bin/ss", &args.map(String::from))
        }
    }
}

/// The process's threads. Nice and affinity are per thread on Linux.
fn threads(pid: u32) -> Result<Vec<u32>, String> {
    let entries = std::fs::read_dir(format!("/proc/{pid}/task")).map_err(|_| format!("process {pid} isn't running"))?;
    let tids: Vec<u32> = entries.filter_map(Result::ok).filter_map(|entry| entry.file_name().to_str()?.parse().ok()).collect();
    if tids.is_empty() { Err(format!("process {pid} isn't running")) } else { Ok(tids) }
}

/// Runs one of the fixed programs with a clean environment, its output going
/// straight to purrtop.
fn pass_through(program: &str, args: &[String]) -> Result<(), String> {
    let status = Command::new(program).args(args).env_clear().env("PATH", "/usr/bin").env("LC_ALL", "C").status().map_err(|err| format!("couldn't run {program}: {err}"))?;
    if status.success() { Ok(()) } else { Err(format!("{program} failed ({status})")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(words: &str) -> Result<Request, String> {
        parse(&words.split_whitespace().map(String::from).collect::<Vec<_>>())
    }

    #[test]
    fn accepts_what_purrtop_sends() {
        assert_eq!(parse_words("signal 15 1234 5678"), Ok(Request::Signal { signal: 15, pids: vec![1234, 5678] }));
        assert_eq!(parse_words("renice -5 1234"), Ok(Request::Renice { nice: -5, pid: 1234 }));
        assert_eq!(parse_words("affinity 0,2,3 1234"), Ok(Request::Affinity { cpus: vec![0, 2, 3], pid: 1234 }));
        assert_eq!(parse_words("sample 1234"), Ok(Request::Sample { pid: 1234 }));
        assert_eq!(parse_words("port-owners"), Ok(Request::PortOwners));
    }

    #[test]
    fn refuses_everything_else() {
        for words in [
            "",
            "signal 15",
            "signal 31 1234",
            "signal 15 0",
            "signal 15 1",
            "signal 15 -1",
            "signal 15 +42",
            "signal 15 12a",
            "renice -21 1234",
            "renice 20 1234",
            "renice 5 1234 5678",
            "affinity 0,99999 1234",
            "affinity 0,,1 1234",
            "affinity 0-3 1234",
            "sample 1234 --output=/etc/passwd",
            "port-owners --kill",
            "exec /bin/sh",
        ] {
            assert!(parse_words(words).is_err(), "should refuse {words:?}");
        }
        let too_many = format!("signal 15 {}", vec!["1234"; MAX_PIDS + 1].join(" "));
        assert!(parse_words(&too_many).is_err());
    }

    #[test]
    fn finds_our_own_threads() {
        assert!(threads(std::process::id()).unwrap().contains(&std::process::id()));
        assert!(threads(u32::MAX).is_err());
    }
}
