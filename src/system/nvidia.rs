//! NVIDIA usage, temperature, power, clocks and memory from NVML, for the
//! opt-in "NVIDIA details" setting.
//!
//! NVML is only opened while sysfs already says the GPU is awake, and it's
//! closed again after each reading. Every reading resets the GPU's idle timer
//! (about 20 seconds on the RTX 4090 Laptop), so when no other program is
//! using the GPU, purrtop waits longer than that before reading again and the
//! GPU can go back to sleep. Measured 2026-10-20: it suspends as soon as it
//! would without purrtop.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use nvml_wrapper::Nvml;
use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::error::NvmlError;

use super::gpu::GpuReading;

/// Longer than the GPU's idle delay, so a reading nobody else needs doesn't keep it awake.
const IDLE_BACKOFF: Duration = Duration::from_secs(30);

/// What NVML read, kept to fill readings while backing off.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Values {
    busy: f32,
    temperature: Option<f32>,
    watts: Option<f32>,
    clock_mhz: Option<f32>,
    max_clock_mhz: Option<f32>,
    vram_used: Option<u64>,
    vram_total: Option<u64>,
}

#[derive(Default)]
pub struct NvidiaReader {
    /// Per PCI slot: the last values and when to read again.
    last: HashMap<String, (Values, Instant)>,
    /// NVML isn't installed or the driver isn't loaded, so stop trying.
    unavailable: bool,
}

impl NvidiaReader {
    /// Fills an awake NVIDIA GPU's reading. Call it only when `reading.awake`.
    pub fn fill(&mut self, reading: &mut GpuReading) {
        if self.unavailable {
            return;
        }
        let now = Instant::now();
        if let Some((values, next)) = self.last.get(&reading.slot)
            && now < *next
        {
            apply(*values, reading);
            return;
        }
        match read(&reading.slot) {
            Ok((values, others)) => {
                let next = if idle(&values, others) { now + IDLE_BACKOFF } else { now };
                self.last.insert(reading.slot.clone(), (values, next));
                apply(values, reading);
            }
            Err(NvmlError::LibloadingError(_) | NvmlError::LibraryNotFound | NvmlError::DriverNotLoaded) => self.unavailable = true,
            // Anything else (a GPU NVML doesn't know, a busy driver) gets another try next second.
            Err(_) => _ = self.last.remove(&reading.slot),
        }
    }

    /// Forgets everything, for when the setting is turned off or on again.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Nothing else is using the GPU and it's doing nothing, so it's only awake
/// because of purrtop's last reading (or about to sleep anyway).
fn idle(values: &Values, others: u32) -> bool {
    others == 0 && values.busy <= 0.
}

fn apply(values: Values, reading: &mut GpuReading) {
    reading.busy = Some(values.busy);
    reading.temperature = values.temperature.or(reading.temperature);
    reading.watts = values.watts.or(reading.watts);
    reading.clock_mhz = values.clock_mhz.or(reading.clock_mhz);
    reading.max_clock_mhz = values.max_clock_mhz.or(reading.max_clock_mhz);
    reading.vram_used = values.vram_used.or(reading.vram_used);
    reading.vram_total = values.vram_total.or(reading.vram_total);
}

/// Opens NVML, reads one GPU and closes it again (dropping `Nvml` shuts it
/// down). Also returns how many other programs are using the GPU.
fn read(slot: &str) -> Result<(Values, u32), NvmlError> {
    let nvml = Nvml::init()?;
    let device = nvml.device_by_pci_bus_id(slot)?;
    let memory = device.memory_info().ok();
    let values = Values {
        busy: (device.utilization_rates()?.gpu as f32 / 100.).clamp(0., 1.),
        temperature: device.temperature(TemperatureSensor::Gpu).ok().map(|celsius| celsius as f32),
        watts: device.power_usage().ok().map(|milliwatts| milliwatts as f32 / 1000.),
        clock_mhz: device.clock_info(Clock::Graphics).ok().map(|mhz| mhz as f32),
        max_clock_mhz: device.max_clock_info(Clock::Graphics).ok().map(|mhz| mhz as f32),
        vram_used: memory.as_ref().map(|memory| memory.used),
        vram_total: memory.as_ref().map(|memory| memory.total),
    };
    let others = device.running_compute_processes_count().unwrap_or(0) + device.running_graphics_processes_count().unwrap_or(0);
    Ok((values, others))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backs_off_only_when_nothing_uses_the_gpu() {
        let quiet = Values::default();
        assert!(idle(&quiet, 0));
        assert!(!idle(&quiet, 1), "another program has it open");
        assert!(!idle(&Values { busy: 0.03, ..quiet }, 0), "busy without showing up as a process, like some games");
    }

    #[test]
    fn keeps_sysfs_values_nvml_doesnt_have() {
        let mut reading = GpuReading {
            name: "GPU".into(),
            vendor: super::super::gpu::Vendor::Nvidia,
            driver: None,
            slot: "0000:01:00.0".into(),
            awake: true,
            power_state: None,
            busy: None,
            temperature: Some(50.),
            watts: None,
            clock_mhz: None,
            max_clock_mhz: None,
            vram_used: None,
            vram_total: None,
        };
        apply(Values { busy: 0.5, watts: Some(12.), vram_total: Some(16 << 30), ..Values::default() }, &mut reading);
        assert_eq!(reading.busy, Some(0.5));
        assert_eq!(reading.temperature, Some(50.));
        assert_eq!(reading.watts, Some(12.));
        assert_eq!(reading.vram_total, Some(16 << 30));
    }
}

#[cfg(test)]
mod live {
    use super::*;

    /// Wakes the dGPU, so it's ignored by default. Run with `cargo test nvidia::live -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn reads_this_machines_nvidia_gpu() {
        let start = Instant::now();
        let (values, others) = read("0000:01:00.0").expect("NVML read");
        println!("{values:?} others={others} in {:?}", start.elapsed());
        let start = Instant::now();
        for _ in 0..10 {
            read("0000:01:00.0").unwrap();
        }
        println!("10 more reads: {:?} each", start.elapsed() / 10);
    }
}
