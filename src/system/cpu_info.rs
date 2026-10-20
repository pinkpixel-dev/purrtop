//! Facts about the processor that don't change while purrtop runs, read once
//! from `/proc/cpuinfo` and `/sys/devices`.

use std::collections::BTreeSet;
use std::path::Path;

/// What kind of core a logical processor sits on. Hybrid Intel chips list
/// their P-cores and E-cores under `/sys/devices/cpu_core` and `cpu_atom`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CoreKind {
    /// Not a hybrid chip, or the kernel doesn't say.
    #[default]
    Unknown,
    Performance,
    Efficiency,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CpuInfo {
    pub model: String,
    pub sockets: usize,
    pub physical_cores: usize,
    pub logical: usize,
    /// One entry per logical processor, in order.
    pub kinds: Vec<CoreKind>,
    /// The advertised base clock. Only some cpufreq drivers report it.
    pub base_mhz: Option<f32>,
    /// `VT-x` or `AMD-V` when the CPU flags offer it.
    pub virtualization: Option<&'static str>,
    /// Set when the kernel says it runs under a hypervisor.
    pub virtual_machine: bool,
    pub driver: Option<String>,
    pub governor: Option<String>,
}

impl CpuInfo {
    pub fn read() -> Self {
        let mut info = Self::parse(&std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default());
        let performance = read_cpu_list(Path::new("/sys/devices/cpu_core/cpus"));
        let efficiency = read_cpu_list(Path::new("/sys/devices/cpu_atom/cpus"));
        info.kinds = core_kinds(info.logical, &performance, &efficiency);
        let freq = Path::new("/sys/devices/system/cpu/cpu0/cpufreq");
        info.base_mhz = super::sensors::read_u64(&freq.join("base_frequency")).map(|khz| khz as f32 / 1000.);
        info.driver = read_trimmed(&freq.join("scaling_driver"));
        info.governor = read_trimmed(&freq.join("scaling_governor"));
        info
    }

    /// How many logical processors sit on each kind of core: (P, E).
    pub fn hybrid_split(&self) -> Option<(usize, usize)> {
        let count = |kind| self.kinds.iter().filter(|&&k| k == kind).count();
        let split = (count(CoreKind::Performance), count(CoreKind::Efficiency));
        (split.0 > 0 && split.1 > 0).then_some(split)
    }

    fn parse(text: &str) -> Self {
        let mut info = Self::default();
        let mut sockets = BTreeSet::new();
        let mut cores = BTreeSet::new();
        let mut socket = String::new();
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "processor" => info.logical += 1,
                "model name" if info.model.is_empty() => info.model = value.to_string(),
                "physical id" => {
                    socket = value.to_string();
                    sockets.insert(socket.clone());
                }
                "core id" => {
                    cores.insert((socket.clone(), value.to_string()));
                }
                "flags" if info.virtualization.is_none() && !info.virtual_machine => {
                    let flags: Vec<&str> = value.split_whitespace().collect();
                    info.virtualization = if flags.contains(&"vmx") {
                        Some("VT-x")
                    } else if flags.contains(&"svm") {
                        Some("AMD-V")
                    } else {
                        None
                    };
                    info.virtual_machine = flags.contains(&"hypervisor");
                }
                _ => {}
            }
        }
        info.sockets = sockets.len().max(1);
        info.physical_cores = if cores.is_empty() { info.logical } else { cores.len() };
        info
    }
}

/// Expands a kernel CPU list like `0-15,20,22-23`.
fn parse_cpu_list(text: &str) -> Vec<usize> {
    let mut cpus = Vec::new();
    for part in text.trim().split(',').filter(|part| !part.is_empty()) {
        match part.split_once('-') {
            Some((start, end)) => {
                if let (Ok(start), Ok(end)) = (start.parse::<usize>(), end.parse::<usize>()) {
                    cpus.extend(start..=end);
                }
            }
            None => cpus.extend(part.parse::<usize>().ok()),
        }
    }
    cpus
}

fn read_cpu_list(path: &Path) -> Vec<usize> {
    std::fs::read_to_string(path).map(|text| parse_cpu_list(&text)).unwrap_or_default()
}

fn core_kinds(logical: usize, performance: &[usize], efficiency: &[usize]) -> Vec<CoreKind> {
    (0..logical)
        .map(|cpu| {
            if performance.contains(&cpu) {
                CoreKind::Performance
            } else if efficiency.contains(&cpu) {
                CoreKind::Efficiency
            } else {
                CoreKind::Unknown
            }
        })
        .collect()
}

fn read_trimmed(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CPUINFO: &str = "\
processor\t: 0
model name\t: Intel(R) Core(TM) i9-14900HX
physical id\t: 0
core id\t\t: 0
flags\t\t: fpu vme vmx sse

processor\t: 1
model name\t: Intel(R) Core(TM) i9-14900HX
physical id\t: 0
core id\t\t: 0
flags\t\t: fpu vme vmx sse

processor\t: 2
model name\t: Intel(R) Core(TM) i9-14900HX
physical id\t: 0
core id\t\t: 8
flags\t\t: fpu vme vmx sse
";

    #[test]
    fn counts_sockets_cores_and_threads() {
        let info = CpuInfo::parse(CPUINFO);
        assert_eq!(info.model, "Intel(R) Core(TM) i9-14900HX");
        assert_eq!((info.sockets, info.physical_cores, info.logical), (1, 2, 3));
        assert_eq!(info.virtualization, Some("VT-x"));
        assert!(!info.virtual_machine);
    }

    #[test]
    fn spots_amd_and_virtual_machines() {
        let info = CpuInfo::parse("processor : 0\nflags : fpu svm hypervisor\n");
        assert_eq!(info.virtualization, Some("AMD-V"));
        assert!(info.virtual_machine);
        // No topology lines: every logical processor counts as a core.
        assert_eq!((info.sockets, info.physical_cores), (1, 1));
    }

    #[test]
    fn expands_cpu_lists() {
        assert_eq!(parse_cpu_list("0-3\n"), vec![0, 1, 2, 3]);
        assert_eq!(parse_cpu_list("0,2,4-5"), vec![0, 2, 4, 5]);
        assert!(parse_cpu_list("").is_empty());
    }

    #[test]
    fn labels_hybrid_cores() {
        let kinds = core_kinds(4, &[0, 1], &[2, 3]);
        assert_eq!(kinds, [CoreKind::Performance, CoreKind::Performance, CoreKind::Efficiency, CoreKind::Efficiency]);
        let info = CpuInfo { kinds, ..CpuInfo::default() };
        assert_eq!(info.hybrid_split(), Some((2, 2)));
        assert_eq!(CpuInfo { kinds: core_kinds(2, &[], &[]), ..CpuInfo::default() }.hybrid_split(), None);
    }
}
