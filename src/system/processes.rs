//! Per-process CPU and memory from `/proc/<pid>/stat`, for the busiest-first list.

use std::collections::HashMap;

/// What `/proc/<pid>/stat` says about one process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcStat {
    pub pid: u32,
    pub name: String,
    /// User plus system time, in clock ticks.
    pub ticks: u64,
    /// Resident memory, in pages.
    pub rss_pages: u64,
}

impl ProcStat {
    /// Parses a stat line. The name sits in parentheses and may itself contain
    /// spaces or parentheses, so fields are counted from the last `)`.
    pub fn parse(text: &str) -> Option<Self> {
        let open = text.find('(')?;
        let close = text.rfind(')')?;
        let pid = text[..open].trim().parse().ok()?;
        let name = text.get(open + 1..close)?.to_string();
        // After the name: state is field 0, utime 11, stime 12, rss 21.
        let fields: Vec<&str> = text.get(close + 1..)?.split_whitespace().collect();
        let number = |ix: usize| fields.get(ix).and_then(|field| field.parse::<u64>().ok());
        Some(Self { pid, name, ticks: number(11)? + number(12)?, rss_pages: number(21)? })
    }
}

/// One row of the busiest-processes list.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessUsage {
    pub pid: u32,
    pub name: String,
    /// Share of the whole machine's CPU time since the last reading, from 0 to 1.
    pub cpu: f32,
    /// Resident memory in bytes.
    pub memory: u64,
}

/// Remembers each process's CPU ticks so the next reading can turn them into usage.
#[derive(Default)]
pub struct ProcessTracker {
    previous: HashMap<u32, u64>,
}

impl ProcessTracker {
    /// Every process with its usage since the last call, busiest first.
    /// `machine_ticks` is how many ticks all CPUs together ran in that time.
    pub fn sample(&mut self, stats: Vec<ProcStat>, machine_ticks: u64, page_size: u64) -> Vec<ProcessUsage> {
        let mut usage: Vec<ProcessUsage> = stats
            .iter()
            .map(|stat| {
                // A process seen for the first time has no earlier reading yet.
                let used = self.previous.get(&stat.pid).map_or(0, |&before| stat.ticks.saturating_sub(before));
                let cpu = if machine_ticks == 0 { 0. } else { (used as f32 / machine_ticks as f32).clamp(0., 1.) };
                ProcessUsage { pid: stat.pid, name: stat.name.clone(), cpu, memory: stat.rss_pages * page_size }
            })
            .collect();
        // Rebuilt every time, so exited processes drop out.
        self.previous = stats.into_iter().map(|stat| (stat.pid, stat.ticks)).collect();
        usage.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.memory.cmp(&a.memory)));
        usage
    }
}

/// Reads every process's stat file. Processes that exit mid-scan are skipped.
pub fn read_all() -> Vec<ProcStat> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_str().is_some_and(|name| name.bytes().all(|b| b.is_ascii_digit())))
        .filter_map(|entry| std::fs::read_to_string(entry.path().join("stat")).ok())
        .filter_map(|text| ProcStat::parse(&text))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_names_with_spaces_and_parens() {
        let line = "1234 (Web Content (x)) S 1 1234 1234 0 -1 4194560 100 0 0 0 150 50 0 0 20 0 30 0 5000 999999 2048 18446744073709551615";
        let stat = ProcStat::parse(line).unwrap();
        assert_eq!(stat, ProcStat { pid: 1234, name: "Web Content (x)".into(), ticks: 200, rss_pages: 2048 });
        assert_eq!(ProcStat::parse("garbage"), None);
    }

    #[test]
    fn usage_is_measured_against_the_whole_machine() {
        let stat = |pid, ticks| ProcStat { pid, name: format!("p{pid}"), ticks, rss_pages: 1 };
        let mut tracker = ProcessTracker::default();
        let first = tracker.sample(vec![stat(1, 100), stat(2, 100)], 400, 4096);
        assert!(first.iter().all(|process| process.cpu == 0.));

        let second = tracker.sample(vec![stat(1, 200), stat(2, 120), stat(3, 50)], 400, 4096);
        assert_eq!(second[0].pid, 1);
        assert_eq!(second[0].cpu, 0.25);
        assert_eq!(second[1].cpu, 0.05);
        assert_eq!(second[0].memory, 4096);
        // New this round, so no usage yet.
        assert_eq!(second.iter().find(|process| process.pid == 3).unwrap().cpu, 0.);
    }
}
