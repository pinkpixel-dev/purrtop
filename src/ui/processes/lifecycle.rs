//! Notices processes starting and exiting between readings, so the table
//! can tint new ones green and keep exited ones on screen, red, for a moment.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::rows::Mark;
use crate::system::processes::ProcessInfo;

/// How long a new or exited process stays highlighted.
const HIGHLIGHT: Duration = Duration::from_millis(2500);

#[derive(Default)]
pub struct Lifecycle {
    /// Every process from the last reading.
    previous: HashMap<u32, ProcessInfo>,
    started: HashMap<u32, Instant>,
    exited: Vec<(ProcessInfo, Instant)>,
    primed: bool,
}

impl Lifecycle {
    /// Forgets everything, so the next reading marks nothing as new. Used
    /// when the page opens again after readings it didn't see.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Records a reading. Returns the highlight for each marked PID and the
    /// recently exited processes to show alongside the live ones.
    pub fn update(&mut self, processes: &[ProcessInfo], now: Instant) -> (HashMap<u32, Mark>, Vec<ProcessInfo>) {
        let current: HashMap<u32, ProcessInfo> = processes.iter().map(|process| (process.pid, process.clone())).collect();
        if self.primed {
            for (pid, process) in &self.previous {
                if !current.contains_key(pid) {
                    let mut ghost = process.clone();
                    ghost.state = 'X';
                    ghost.cpu = 0.;
                    self.exited.push((ghost, now));
                }
            }
            for pid in current.keys() {
                if !self.previous.contains_key(pid) {
                    self.started.insert(*pid, now);
                }
            }
        }
        self.primed = true;
        self.previous = current;

        let fresh = |at: &Instant| now.duration_since(*at) < HIGHLIGHT;
        self.started.retain(|pid, at| fresh(at) && self.previous.contains_key(pid));
        // A PID can come back quickly; the live process wins.
        self.exited.retain(|(ghost, at)| fresh(at) && !self.previous.contains_key(&ghost.pid));

        let mut marks: HashMap<u32, Mark> = self.started.keys().map(|&pid| (pid, Mark::New)).collect();
        marks.extend(self.exited.iter().map(|(ghost, _)| (ghost.pid, Mark::Exited)));
        (marks, self.exited.iter().map(|(ghost, _)| ghost.clone()).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::processes::Kind;
    use crate::ui::processes::rows::tests::process;

    #[test]
    fn marks_new_and_exited_processes_for_a_moment() {
        let start = Instant::now();
        let a = process(1, 0, "a", Kind::Background);
        let b = process(2, 0, "b", Kind::Background);
        let c = process(3, 0, "c", Kind::Background);
        let mut lifecycle = Lifecycle::default();

        // The first reading only primes it.
        let (marks, ghosts) = lifecycle.update(&[a.clone(), b.clone()], start);
        assert!(marks.is_empty() && ghosts.is_empty());

        let (marks, ghosts) = lifecycle.update(&[a.clone(), c.clone()], start + Duration::from_secs(1));
        assert_eq!(marks.get(&3), Some(&Mark::New));
        assert_eq!(marks.get(&2), Some(&Mark::Exited));
        assert_eq!(ghosts.len(), 1);
        assert_eq!((ghosts[0].pid, ghosts[0].state), (2, 'X'));

        let (marks, ghosts) = lifecycle.update(&[a, c], start + Duration::from_secs(5));
        assert!(marks.is_empty() && ghosts.is_empty());
    }
}
