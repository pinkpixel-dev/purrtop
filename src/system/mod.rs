//! Everything purrtop reads about the machine, straight from `/proc` and `/sys`.

pub mod control;
pub mod cpu;
pub mod cpu_info;
pub mod disk_info;
pub mod details;
pub mod format;
pub mod gpu;
pub mod history;
pub mod io;
pub mod memory;
pub mod monitor;
pub mod net_info;
pub mod pressure;
pub mod processes;
pub mod sensors;
pub mod users;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cpu::StatTimes;
use cpu_info::CpuInfo;
use gpu::{GpuDevice, GpuReading};
use disk_info::DiskInfo;
use io::{Disk, DiskCounters, DiskRates, Interface, NetCounters, NetRates, rates_by_name};
use net_info::InterfaceInfo;
use memory::Memory;
use pressure::Pressure;
use processes::{ProcessInfo, ProcessScanner};
use sensors::Clock;

/// How many processes the busiest-first list keeps.
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
    /// The kernel's part of each entry in `cores`.
    pub core_kernel: Vec<f32>,
    /// Read once at startup and shared by every snapshot.
    pub cpu_info: Arc<CpuInfo>,
    pub clock: Option<Clock>,
    /// CPU package temperature in °C.
    pub temperature: Option<f32>,
    pub memory: Memory,
    pub pressure: Option<Pressure>,
    /// Every physical interface added together.
    pub network: NetRates,
    /// Every disk added together, with the busiest disk's busy share.
    pub disk: DiskRates,
    pub interfaces: Vec<Interface>,
    /// Sorted by kernel name, so `Disk 0` stays the same disk.
    pub disks: Vec<Disk>,
    pub gpus: Vec<GpuReading>,
    /// Every process. Shared, since pages hold on to it between readings.
    pub processes: Arc<Vec<ProcessInfo>>,
    /// Indices into `processes`, busiest first, at most [`TOP_PROCESSES`].
    pub top: Vec<usize>,
    pub uptime: Duration,
    /// 1, 5 and 15 minute load averages.
    pub load: [f32; 3],
}

/// Takes snapshots. It remembers the last set of counters, since CPU use and
/// I/O rates are the change between two readings.
pub struct Reader {
    previous: StatTimes,
    net: Vec<NetCounters>,
    disk: Vec<DiskCounters>,
    /// Read the first time each disk shows up.
    disk_info: HashMap<String, DiskInfo>,
    taken: Instant,
    processes: ProcessScanner,
    temperature: Option<PathBuf>,
    gpus: Vec<GpuDevice>,
    cpu_info: Arc<CpuInfo>,
}

impl Reader {
    pub fn new() -> Self {
        let pci_ids = std::fs::read_to_string("/usr/share/hwdata/pci.ids").unwrap_or_default();
        let page_size = page_size();
        let mut processes = ProcessScanner::new(page_size, ticks_per_second());
        // Prime the scanner so the first snapshot already has per-process usage.
        processes.scan(false, 0, 0.);
        Self {
            previous: StatTimes::read().unwrap_or_default(),
            net: NetCounters::read().unwrap_or_default(),
            disk: DiskCounters::read().unwrap_or_default(),
            disk_info: HashMap::new(),
            taken: Instant::now(),
            processes,
            temperature: sensors::find_cpu_temperature(),
            gpus: GpuDevice::discover(&pci_ids),
            cpu_info: Arc::new(CpuInfo::read()),
        }
    }

    /// `detailed` also reads each process's status, I/O and open files.
    pub fn sample(&mut self, detailed: bool) -> std::io::Result<Snapshot> {
        let stat = StatTimes::read()?;
        let now = Instant::now();
        let elapsed = now.duration_since(self.taken);

        let cpu = stat.all.usage_since(self.previous.all);
        let kernel = stat.all.kernel_since(self.previous.all);
        let (cores, core_kernel) = stat
            .cores
            .iter()
            .enumerate()
            .map(|(ix, core)| {
                self.previous.cores.get(ix).map_or((0., 0.), |&earlier| (core.usage_since(earlier), core.kernel_since(earlier)))
            })
            .unzip();
        let machine_ticks = stat.all.total.saturating_sub(self.previous.all.total);
        self.previous = stat;

        let net = NetCounters::read().unwrap_or_else(|| self.net.clone());
        let disk = DiskCounters::read().unwrap_or_else(|| self.disk.clone());
        let net_rates = rates_by_name(&self.net, &net, |counters| &counters.name, |a, b| NetRates::between(a, b, elapsed));
        let disk_rates = rates_by_name(&self.disk, &disk, |counters| &counters.name, |a, b| DiskRates::between(a, b, elapsed));
        let network = net_rates.iter().fold(NetRates::default(), |total, rates| NetRates {
            received: total.received + rates.received,
            sent: total.sent + rates.sent,
        });
        let combined_disk = DiskRates::combined(disk_rates.iter().copied());
        let interfaces = self.interfaces(&net, net_rates);
        let disks = self.disks(&disk, disk_rates);
        (self.net, self.disk, self.taken) = (net, disk, now);

        let processes = self.processes.scan(detailed, machine_ticks, elapsed.as_secs_f64());
        let mut top: Vec<usize> = (0..processes.len()).collect();
        top.sort_by(|&a, &b| processes[b].cpu.total_cmp(&processes[a].cpu).then(processes[b].memory.cmp(&processes[a].memory)));
        top.truncate(TOP_PROCESSES);

        Ok(Snapshot {
            cpu,
            kernel,
            cores,
            core_kernel,
            cpu_info: self.cpu_info.clone(),
            clock: sensors::read_clock(),
            temperature: self.temperature.as_deref().and_then(sensors::read_celsius),
            memory: Memory::read()?,
            pressure: Pressure::read(),
            network,
            disk: combined_disk,
            interfaces,
            disks,
            gpus: self.gpus.iter().map(GpuDevice::read).collect(),
            processes: Arc::new(processes),
            top,
            uptime: read_uptime().unwrap_or_default(),
            load: read_load().unwrap_or_default(),
        })
    }
}

impl Reader {
    fn interfaces(&self, counters: &[NetCounters], rates: Vec<NetRates>) -> Vec<Interface> {
        let mut addresses = net_info::read_addresses();
        counters
            .iter()
            .zip(rates)
            .map(|(counters, rates)| Interface {
                name: counters.name.clone(),
                info: InterfaceInfo::read(&counters.name, &mut addresses),
                rates,
                total_received: counters.received,
                total_sent: counters.sent,
            })
            .collect()
    }

    fn disks(&mut self, counters: &[DiskCounters], rates: Vec<DiskRates>) -> Vec<Disk> {
        let names: Vec<String> = counters.iter().map(|counters| counters.name.clone()).collect();
        let mut volumes = disk_info::read_volumes(&names);
        let mut disks: Vec<Disk> = counters
            .iter()
            .zip(rates)
            .map(|(counters, rates)| {
                let name = counters.name.clone();
                let info = self.disk_info.entry(name.clone()).or_insert_with(|| DiskInfo::read(&name)).clone();
                let (mine, rest) = volumes.drain(..).partition(|volume| volume.disk == name);
                volumes = rest;
                Disk { name, info, rates, volumes: mine }
            })
            .collect();
        disks.sort_by(|a, b| a.name.cmp(&b.name));
        // Forget disks that were unplugged, so a new one with the same name is read fresh.
        self.disk_info.retain(|name, _| names.contains(name));
        disks
    }
}

fn page_size() -> u64 {
    // SAFETY: sysconf only reads a system constant.
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if size > 0 { size as u64 } else { 4096 }
}

fn ticks_per_second() -> f64 {
    // SAFETY: sysconf only reads a system constant.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks > 0 { ticks as f64 } else { 100. }
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
        let snapshot = reader.sample(true).expect("/proc should be readable on Linux");
        assert!(!snapshot.cores.is_empty());
        assert!(snapshot.memory.total > 0);
        assert!(!snapshot.processes.is_empty());
        assert!(!snapshot.top.is_empty() && snapshot.top.len() <= TOP_PROCESSES);
        assert!((0. ..=1.).contains(&snapshot.cpu));
        assert!(snapshot.kernel <= snapshot.cpu);
    }
}
