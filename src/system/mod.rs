//! Everything purrtop reads about the machine, straight from `/proc`.

pub mod cpu;
pub mod format;
pub mod history;
pub mod memory;
pub mod monitor;

use std::time::Duration;

use cpu::StatTimes;
use memory::Memory;

/// One reading of the whole machine.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Whole-machine CPU use, from 0 to 1.
    pub cpu: f32,
    /// CPU use per logical processor, from 0 to 1.
    pub cores: Vec<f32>,
    pub memory: Memory,
    pub processes: usize,
    pub uptime: Duration,
    /// 1, 5 and 15 minute load averages.
    pub load: [f32; 3],
}

/// Takes snapshots, remembering the last CPU counters so each reading is the
/// use since the one before it.
pub struct Reader {
    previous: StatTimes,
}

impl Reader {
    pub fn new() -> Self {
        Self { previous: StatTimes::read().unwrap_or_default() }
    }

    pub fn sample(&mut self) -> std::io::Result<Snapshot> {
        let stat = StatTimes::read()?;
        let cpu = stat.all.usage_since(self.previous.all);
        let cores = stat
            .cores
            .iter()
            .enumerate()
            .map(|(ix, core)| self.previous.cores.get(ix).map_or(0., |earlier| core.usage_since(*earlier)))
            .collect();
        self.previous = stat;

        Ok(Snapshot {
            cpu,
            cores,
            memory: Memory::read()?,
            processes: count_processes(),
            uptime: read_uptime().unwrap_or_default(),
            load: read_load().unwrap_or_default(),
        })
    }
}

/// Every numeric directory in `/proc` is a process.
fn count_processes() -> usize {
    std::fs::read_dir("/proc")
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_str().is_some_and(|name| name.bytes().all(|b| b.is_ascii_digit())))
                .count()
        })
        .unwrap_or(0)
}

fn read_uptime() -> Option<Duration> {
    parse_uptime(&std::fs::read_to_string("/proc/uptime").ok()?)
}

fn parse_uptime(text: &str) -> Option<Duration> {
    let seconds: f64 = text.split_whitespace().next()?.parse().ok()?;
    Some(Duration::from_secs_f64(seconds.max(0.)))
}

fn read_load() -> Option<[f32; 3]> {
    parse_load(&std::fs::read_to_string("/proc/loadavg").ok()?)
}

fn parse_load(text: &str) -> Option<[f32; 3]> {
    let mut fields = text.split_whitespace().map(|field| field.parse::<f32>().ok());
    Some([fields.next()??, fields.next()??, fields.next()??])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_uptime_and_load() {
        assert_eq!(parse_uptime("1016.30 30000.00\n"), Some(Duration::from_secs_f64(1016.30)));
        assert_eq!(parse_uptime(""), None);
        assert_eq!(parse_load("0.52 0.61 0.70 2/1234 5678\n"), Some([0.52, 0.61, 0.70]));
        assert_eq!(parse_load("0.52"), None);
    }

    #[test]
    fn reads_this_machine() {
        let mut reader = Reader::new();
        let snapshot = reader.sample().expect("/proc should be readable on Linux");
        assert!(!snapshot.cores.is_empty());
        assert!(snapshot.memory.total > 0);
        assert!(snapshot.processes > 0);
        assert!((0. ..=1.).contains(&snapshot.cpu));
    }
}
