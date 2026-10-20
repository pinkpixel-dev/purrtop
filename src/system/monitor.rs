//! The live feed every page reads from: a snapshot each second, off the UI
//! thread, plus short histories for charts.

use std::time::Duration;

use gpui_kit::*;

use super::history::History;
use super::{Reader, Snapshot};

/// How often the machine is sampled.
pub const SAMPLE_INTERVAL: Duration = Duration::from_secs(1);

pub struct Monitor {
    latest: Option<Snapshot>,
    pub cpu: History,
    pub memory: History,
    /// Set when the last sample failed, cleared by the next good one.
    pub error: Option<SharedString>,
}

impl Monitor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            let mut reader = cx.background_spawn(async { Reader::new() }).await;
            loop {
                cx.background_executor().timer(SAMPLE_INTERVAL).await;
                let (back, result) = cx
                    .background_spawn(async move {
                        let result = reader.sample();
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

        Self { latest: None, cpu: History::default(), memory: History::default(), error: None }
    }

    /// `None` until the first sample lands, about a second after launch.
    pub fn latest(&self) -> Option<&Snapshot> {
        self.latest.as_ref()
    }

    fn record(&mut self, result: std::io::Result<Snapshot>) {
        match result {
            Ok(snapshot) => {
                self.cpu.push(snapshot.cpu);
                self.memory.push(snapshot.memory.used_fraction());
                self.latest = Some(snapshot);
                self.error = None;
            }
            Err(err) => self.error = Some(format!("Couldn't read /proc: {err}").into()),
        }
    }
}
