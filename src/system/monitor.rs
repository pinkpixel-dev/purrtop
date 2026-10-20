//! The live feed every page reads from: a snapshot each second, off the UI
//! thread, plus short histories for charts.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui_kit::*;

use super::history::History;
use super::{Reader, Snapshot};

/// How often the machine is sampled.
pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

/// One disk's busy share and read and write rates.
#[derive(Default)]
pub struct DiskHistory {
    pub busy: History,
    pub read: History,
    pub written: History,
}

/// One interface's receive and send rates.
#[derive(Default)]
pub struct NetHistory {
    pub received: History,
    pub sent: History,
}

/// One GPU's busy share, clock in MHz and temperature in °C. Readings the
/// driver doesn't give stay at zero.
#[derive(Default)]
pub struct GpuHistory {
    pub busy: History,
    pub clock: History,
    pub temperature: History,
}

/// One battery's charge (0 to 1) and power in watts.
#[derive(Default)]
pub struct BatteryHistory {
    pub charge: History,
    pub watts: History,
}

pub struct Monitor {
    latest: Option<Snapshot>,
    /// Each history holds raw readings: fractions, °C or bytes per second.
    pub cpu: History,
    pub kernel: History,
    pub temperature: History,
    pub memory: History,
    pub swap: History,
    /// One history per logical processor, and the kernel's part of each.
    pub cores: Vec<History>,
    pub core_kernel: Vec<History>,
    pub received: History,
    pub sent: History,
    pub disk_read: History,
    pub disk_written: History,
    /// Per disk and per interface, by kernel name.
    pub disks: HashMap<String, DiskHistory>,
    pub interfaces: HashMap<String, NetHistory>,
    /// Per GPU by PCI slot, per sensor by key, per battery by name.
    pub gpus: HashMap<String, GpuHistory>,
    pub thermals: HashMap<String, History>,
    pub batteries: HashMap<String, BatteryHistory>,
    /// Set when the last sample failed, cleared by the next good one.
    pub error: Option<SharedString>,
    /// Shared with the sampling loop: read per-process details too.
    detailed: Arc<AtomicBool>,
}

impl Monitor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let detailed = Arc::new(AtomicBool::new(false));
        let flag = detailed.clone();
        cx.spawn(async move |this, cx| {
            let mut reader = cx.background_spawn(async { Reader::new() }).await;
            loop {
                cx.background_executor().timer(SAMPLE_INTERVAL).await;
                let detailed = flag.load(Ordering::Relaxed);
                let (back, result) = cx
                    .background_spawn(async move {
                        let result = reader.sample(detailed);
                        (reader, result)
                    })
                    .await;
                reader = back;
                let updated = this.update(cx, |monitor, cx| {
                    monitor.record(result);
                    cx.notify();
                });
                // The monitor was dropped with its window.
                if updated.is_err() {
                    break;
                }
            }
        })
        .detach();

        Self {
            latest: None,
            cpu: History::default(),
            kernel: History::default(),
            temperature: History::default(),
            memory: History::default(),
            swap: History::default(),
            cores: Vec::new(),
            core_kernel: Vec::new(),
            received: History::default(),
            sent: History::default(),
            disk_read: History::default(),
            disk_written: History::default(),
            disks: HashMap::new(),
            interfaces: HashMap::new(),
            gpus: HashMap::new(),
            thermals: HashMap::new(),
            batteries: HashMap::new(),
            error: None,
            detailed,
        }
    }

    /// Turns per-process details (status, I/O, open files) on or off. They
    /// cost several extra files per process, so only the Processes page asks.
    pub fn set_detailed(&self, detailed: bool) {
        self.detailed.store(detailed, Ordering::Relaxed);
    }

    /// `None` until the first sample lands, about a second after launch.
    pub fn latest(&self) -> Option<&Snapshot> {
        self.latest.as_ref()
    }

    fn record(&mut self, result: std::io::Result<Snapshot>) {
        match result {
            Ok(snapshot) => {
                self.cpu.push(snapshot.cpu);
                self.kernel.push(snapshot.kernel);
                if let Some(celsius) = snapshot.temperature {
                    self.temperature.push(celsius);
                }
                self.memory.push(snapshot.memory.used_fraction());
                self.swap.push(snapshot.memory.swap_fraction());
                push_each(&mut self.cores, &snapshot.cores);
                push_each(&mut self.core_kernel, &snapshot.core_kernel);
                self.received.push(snapshot.network.received as f32);
                self.sent.push(snapshot.network.sent as f32);
                self.disk_read.push(snapshot.disk.read as f32);
                self.disk_written.push(snapshot.disk.written as f32);
                self.disks.retain(|name, _| snapshot.disks.iter().any(|disk| disk.name == *name));
                for disk in &snapshot.disks {
                    let history = self.disks.entry(disk.name.clone()).or_default();
                    history.busy.push(disk.rates.busy);
                    history.read.push(disk.rates.read as f32);
                    history.written.push(disk.rates.written as f32);
                }
                self.interfaces.retain(|name, _| snapshot.interfaces.iter().any(|interface| interface.name == *name));
                for interface in &snapshot.interfaces {
                    let history = self.interfaces.entry(interface.name.clone()).or_default();
                    history.received.push(interface.rates.received as f32);
                    history.sent.push(interface.rates.sent as f32);
                }
                for gpu in &snapshot.gpus {
                    let history = self.gpus.entry(gpu.slot.clone()).or_default();
                    // A sleeping GPU is idle, so its lines drop to zero instead of freezing.
                    history.busy.push(gpu.busy.unwrap_or(0.));
                    history.clock.push(gpu.clock_mhz.unwrap_or(0.));
                    history.temperature.push(gpu.temperature.unwrap_or(0.));
                }
                self.thermals.retain(|key, _| snapshot.thermals.iter().any(|reading| reading.key == *key));
                for reading in &snapshot.thermals {
                    self.thermals.entry(reading.key.clone()).or_default().push(reading.celsius);
                }
                self.batteries.retain(|name, _| snapshot.batteries.iter().any(|battery| battery.name == *name));
                for battery in &snapshot.batteries {
                    let history = self.batteries.entry(battery.name.clone()).or_default();
                    history.charge.push(battery.charge);
                    history.watts.push(battery.watts.unwrap_or(0.));
                }
                self.latest = Some(snapshot);
                self.error = None;
            }
            Err(err) => self.error = Some(format!("Couldn't read /proc: {err}").into()),
        }
    }
}

/// Pushes one reading into each history, growing the list if a CPU came online.
fn push_each(histories: &mut Vec<History>, values: &[f32]) {
    if histories.len() < values.len() {
        histories.resize_with(values.len(), History::default);
    }
    for (history, &value) in histories.iter_mut().zip(values) {
        history.push(value);
    }
}
