//! The live feed every page reads from: a snapshot each second, off the UI
//! thread, plus short histories for charts.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui_kit::*;

use super::history::History;
use super::{Reader, Snapshot};

/// How often the machine is sampled.
pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

pub struct Monitor {
    latest: Option<Snapshot>,
    /// Each history holds raw readings: fractions, °C or bytes per second.
    pub cpu: History,
    pub kernel: History,
    pub temperature: History,
    pub memory: History,
    pub received: History,
    pub sent: History,
    pub disk_read: History,
    pub disk_written: History,
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
            received: History::default(),
            sent: History::default(),
            disk_read: History::default(),
            disk_written: History::default(),
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
                self.received.push(snapshot.network.received as f32);
                self.sent.push(snapshot.network.sent as f32);
                self.disk_read.push(snapshot.disk.read as f32);
                self.disk_written.push(snapshot.disk.written as f32);
                self.latest = Some(snapshot);
                self.error = None;
            }
            Err(err) => self.error = Some(format!("Couldn't read /proc: {err}").into()),
        }
    }
}
