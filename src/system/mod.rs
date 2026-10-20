//! Everything purrtop reads about the machine, straight from `/proc` and `/sys`.

pub mod cpu;
pub mod format;
pub mod gpu;
pub mod history;
pub mod io;
pub mod memory;
pub mod monitor;
pub mod pressure;
pub mod processes;
pub mod sensors;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use cpu::StatTimes;
use gpu::{GpuDevice, GpuReading};
use io::{DiskCounters, DiskRates, NetCounters, NetRates};
use memory::Memory;
use pressure::Pressure;
use processes::{ProcessTracker, ProcessUsage};
use sensors::Clock;

/// How many processes a snapshot keeps, busiest first.
pub const TOP_PROCESSES: usize = 64;

/// One reading of the whole machine.
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    /// Whole-machine CPU use, from 0 to 1.
    pub cpu: f32,
    /// The kernel's part of `cpu`, from 0 to 1.
    pub kernel: f32,
    /// CPU use per logical processor, from 0 to 1.
    pub cores: Vec<f32>,
    pub clock: Option<Clock>,
    /// CPU package temperature in °C.
    pub temperature: Option<f32>,
    pub memory: Memory,
    pub pressure: Option<Pressure>,
    pub network: NetRates,
    pub disk: DiskRates,
    pub gpus: Vec<GpuReading>,
    /// Busiest first, at most [`TOP_PROCESSES`].
    pub top: Vec<ProcessUsage>,
    pub processes: usize,
    pub uptime: Duration,
    /// 1, 5 and 15 minute load averages.
    pub load: [f32; 3],
}

/// Takes snapshots. It remembers the last set of counters, since CPU use and
/// I/O rates are the change between two readings.
pub struct Reader {
    previous: StatTimes,
    net: NetCounters,
    disk: DiskCounters,
    taken: Instant,
    processes: ProcessTracker,
    temperature: Option<PathBuf>,
    gpus: Vec<GpuDevice>,
    page_size: u64,
}

impl Reader {
    pub fn new() -> Self {
        let pci_ids = std::fs::read_to_string("/usr/share/hwdata/pci.ids").unwrap_or_default();
        let mut processes = ProcessTracker::default();
        // Prime the tracker so the first snapshot already has per-process usage.
        processes.sample(processes::read_all(), 0, 1);
        Self {
            previous: StatTimes::read().unwrap_or_default(),
            net: NetCounters::read().unwrap_or_default(),
            disk: DiskCounters::read().unwrap_or_default(),
            taken: Instant::now(),
            processes,
            temperature: sensors::find_cpu_temperature(),
            gpus: GpuDevice::discover(&pci_ids),
            page_size: page_size(),
        }
    }

    pub fn sample(&mut self) -> std::io::Result<Snapshot> {
        let stat = StatTimes::read()?;
        let now = Instant::now();
        let elapsed = now.duration_since(self.taken);

        let cpu = stat.all.usage_since(self.previous.all);
        let kernel = stat.all.kernel_since(self.previous.all);
        let cores = stat
            .cores
            .iter()
            .enumerate()
            .map(|(ix, core)| self.previous.cores.get(ix).map_or(0., |earlier| core.usage_since(*earlier)))
            .collect();
        let machine_ticks = stat.all.total.saturating_sub(self.previous.all.total);
        self.previous = stat;

        let net = NetCounters::read().unwrap_or(self.net);
        let disk = DiskCounters::read().unwrap_or(self.disk);
        let network = NetRates::between(self.net, net, elapsed);
        let disk_rates = DiskRates::between(self.disk, disk, elapsed);
        (self.net, self.disk, self.taken) = (net, disk, now);

        let all = processes::read_all();
        let processes = all.len();
        let mut top = self.processes.sample(all, machine_ticks, self.page_size);
        top.truncate(TOP_PROCESSES);

        Ok(Snapshot {
            cpu,
            kernel,
            cores,
            clock: sensors::read_clock(),
            temperature: self.temperature.as_deref().and_then(sensors::read_celsius),
            memory: Memory::read()?,
            pressure: Pressure::read(),
            network,
            disk: disk_rates,
            gpus: self.gpus.iter().map(GpuDevice::read).collect(),
            top,
            processes,
            uptime: read_uptime().unwrap_or_default(),
            load: read_load().unwrap_or_default(),
        })
    }
}

fn page_size() -> u64 {
    // SAFETY: sysconf only reads a system constant.
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if size > 0 { size as u64 } else { 4096 }
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
        std::thread::sleep(Duration::from_millis(200));
        let snapshot = reader.sample().expect("/proc should be readable on Linux");
        assert!(!snapshot.cores.is_empty());
        assert!(snapshot.memory.total > 0);
        assert!(snapshot.processes > 0);
        assert!(!snapshot.top.is_empty() && snapshot.top.len() <= TOP_PROCESSES);
        assert!((0. ..=1.).contains(&snapshot.cpu));
        assert!(snapshot.kernel <= snapshot.cpu);
    }
}
