//! Pressure stall information from `/proc/pressure`: how much of the last 10
//! seconds tasks spent waiting on CPU, memory or I/O.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pressure {
    /// Each is the "some" 10 second average, from 0 to 1.
    pub cpu: f32,
    pub memory: f32,
    pub io: f32,
}

impl Pressure {
    /// `None` on kernels built without PSI.
    pub fn read() -> Option<Self> {
        let read = |name: &str| std::fs::read_to_string(format!("/proc/pressure/{name}")).ok().and_then(|text| parse_some(&text));
        Some(Self { cpu: read("cpu")?, memory: read("memory")?, io: read("io")? })
    }

    /// The worst of the three.
    pub fn highest(&self) -> f32 {
        self.cpu.max(self.memory).max(self.io)
    }
}

/// Reads `avg10` from the `some` line.
fn parse_some(text: &str) -> Option<f32> {
    let line = text.lines().find(|line| line.starts_with("some "))?;
    let value = line.split_whitespace().find_map(|field| field.strip_prefix("avg10="))?;
    Some((value.parse::<f32>().ok()? / 100.).clamp(0., 1.))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_some_average() {
        let text = "some avg10=41.00 avg60=36.94 avg300=40.75 total=22758389710\n\
                    full avg10=40.18 avg60=35.94 avg300=39.52 total=20689525150\n";
        assert_eq!(parse_some(text), Some(0.41));
        assert_eq!(parse_some("full avg10=1.00"), None);
        assert_eq!(parse_some(""), None);
    }
}
