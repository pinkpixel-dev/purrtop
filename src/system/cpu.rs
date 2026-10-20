//! CPU load from `/proc/stat`.

/// Cumulative CPU time in clock ticks: busy, the kernel's part of busy, and total.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuTimes {
    pub busy: u64,
    /// System, IRQ and soft IRQ time.
    pub kernel: u64,
    pub total: u64,
}

impl CpuTimes {
    /// The share of time spent busy between `earlier` and `self`, from 0 to 1.
    pub fn usage_since(self, earlier: CpuTimes) -> f32 {
        share(self.busy, earlier.busy, self.total, earlier.total)
    }

    /// The share of time spent in the kernel between `earlier` and `self`, from 0 to 1.
    pub fn kernel_since(self, earlier: CpuTimes) -> f32 {
        share(self.kernel, earlier.kernel, self.total, earlier.total)
    }

    /// Reads one `cpu…` line. Guest time is already counted in user time, so
    /// only the first eight fields add up to the total.
    fn parse(fields: &str) -> Option<Self> {
        let values: Vec<u64> = fields.split_whitespace().take(8).map(|field| field.parse().ok()).collect::<Option<_>>()?;
        if values.len() < 4 {
            return None;
        }
        let get = |ix: usize| values.get(ix).copied().unwrap_or(0);
        let idle = get(3) + get(4);
        let kernel = get(2) + get(5) + get(6);
        let busy = get(0) + get(1) + kernel + get(7);
        Some(Self { busy, kernel, total: busy + idle })
    }
}

/// How much of the `total` gap the `part` gap covers. Counters that went
/// backwards (CPU hotplug) count as zero instead of going negative.
fn share(part: u64, part_before: u64, total: u64, total_before: u64) -> f32 {
    let total = total.saturating_sub(total_before);
    if total == 0 {
        return 0.;
    }
    (part.saturating_sub(part_before) as f32 / total as f32).clamp(0., 1.)
}

/// The whole-machine line plus one entry per logical CPU, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StatTimes {
    pub all: CpuTimes,
    pub cores: Vec<CpuTimes>,
}

impl StatTimes {
    pub fn parse(text: &str) -> Self {
        let mut times = Self::default();
        for line in text.lines() {
            let Some(rest) = line.strip_prefix("cpu") else {
                continue;
            };
            if let Some(fields) = rest.strip_prefix(' ') {
                times.all = CpuTimes::parse(fields).unwrap_or_default();
            } else if let Some((_, fields)) = rest.split_once(' ') {
                times.cores.extend(CpuTimes::parse(fields));
            }
        }
        times
    }

    pub fn read() -> std::io::Result<Self> {
        Ok(Self::parse(&std::fs::read_to_string("/proc/stat")?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &str = "cpu  100 0 50 800 50 0 0 0 0 0\n\
                        cpu0 60 0 20 400 20 0 0 0 0 0\n\
                        cpu1 40 0 30 400 30 0 0 0 0 0\n\
                        intr 12345\n\
                        ctxt 999\n";

    #[test]
    fn reads_the_total_and_every_core() {
        let times = StatTimes::parse(STAT);
        assert_eq!(times.all, CpuTimes { busy: 150, kernel: 50, total: 1000 });
        assert_eq!(times.cores.len(), 2);
        assert_eq!(times.cores[1], CpuTimes { busy: 70, kernel: 30, total: 500 });
    }

    #[test]
    fn usage_is_the_busy_share_of_the_gap() {
        let earlier = CpuTimes { busy: 100, kernel: 40, total: 1000 };
        let later = CpuTimes { busy: 150, kernel: 60, total: 1200 };
        assert!((later.usage_since(earlier) - 0.25).abs() < f32::EPSILON);
        assert!((later.kernel_since(earlier) - 0.1).abs() < f32::EPSILON);
        assert_eq!(earlier.usage_since(earlier), 0.);
        // A counter that went backwards (CPU hotplug) never goes negative.
        assert_eq!(earlier.usage_since(later), 0.);
    }
}
