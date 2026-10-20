//! Memory and swap from `/proc/meminfo`.

/// Sizes in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Memory {
    pub total: u64,
    pub available: u64,
    pub cached: u64,
    pub swap_total: u64,
    pub swap_free: u64,
    /// Memory the kernel has promised to processes, and how far it lets that go.
    pub committed: u64,
    pub commit_limit: u64,
}

impl Memory {
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    /// Used memory as a share of the total, from 0 to 1.
    pub fn used_fraction(&self) -> f32 {
        if self.total == 0 { 0. } else { self.used() as f32 / self.total as f32 }
    }

    pub fn swap_used(&self) -> u64 {
        self.swap_total.saturating_sub(self.swap_free)
    }

    /// Used swap as a share of all swap, from 0 to 1.
    pub fn swap_fraction(&self) -> f32 {
        if self.swap_total == 0 { 0. } else { self.swap_used() as f32 / self.swap_total as f32 }
    }

    /// Memory that's neither in use nor cache.
    pub fn free(&self) -> u64 {
        self.total.saturating_sub(self.used()).saturating_sub(self.cached)
    }

    pub fn parse(text: &str) -> Self {
        let mut memory = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let Some(bytes) = value.split_whitespace().next().and_then(|kb| kb.parse::<u64>().ok()).map(|kb| kb * 1024) else {
                continue;
            };
            match key {
                "MemTotal" => memory.total = bytes,
                "MemAvailable" => memory.available = bytes,
                // The kernel's "cache" in tools like free: page cache plus reclaimable slab.
                "Cached" | "SReclaimable" => memory.cached += bytes,
                "SwapTotal" => memory.swap_total = bytes,
                "SwapFree" => memory.swap_free = bytes,
                "Committed_AS" => memory.committed = bytes,
                "CommitLimit" => memory.commit_limit = bytes,
                _ => {}
            }
        }
        memory
    }

    pub fn read() -> std::io::Result<Self> {
        Ok(Self::parse(&std::fs::read_to_string("/proc/meminfo")?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_meminfo_in_bytes() {
        let memory = Memory::parse(
            "MemTotal:       32000000 kB\n\
             MemFree:         1000000 kB\n\
             MemAvailable:   24000000 kB\n\
             Cached:         10000000 kB\n\
             SReclaimable:    1000000 kB\n\
             SwapTotal:       4000000 kB\n\
             SwapFree:        3000000 kB\n\
             CommitLimit:    20000000 kB\n\
             Committed_AS:   12000000 kB\n\
             HugePages_Total:       0\n",
        );
        assert_eq!(memory.total, 32_000_000 * 1024);
        assert_eq!(memory.used(), 8_000_000 * 1024);
        assert_eq!(memory.cached, 11_000_000 * 1024);
        assert_eq!(memory.swap_used(), 1_000_000 * 1024);
        assert!((memory.used_fraction() - 0.25).abs() < f32::EPSILON);
        assert!((memory.swap_fraction() - 0.25).abs() < f32::EPSILON);
        assert_eq!(memory.free(), 13_000_000 * 1024);
        assert_eq!((memory.committed, memory.commit_limit), (12_000_000 * 1024, 20_000_000 * 1024));
    }

    #[test]
    fn empty_input_is_all_zero() {
        assert_eq!(Memory::parse(""), Memory::default());
        assert_eq!(Memory::default().used_fraction(), 0.);
        assert_eq!(Memory::default().swap_fraction(), 0.);
    }
}
