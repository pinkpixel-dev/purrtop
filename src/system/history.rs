//! A fixed-length run of recent readings for charts.

use std::collections::VecDeque;

/// How many samples a history keeps: two minutes at one sample a second.
pub const HISTORY_LEN: usize = 120;

#[derive(Clone, Debug, Default)]
pub struct History {
    values: VecDeque<f32>,
}

impl History {
    /// Adds a reading, dropping the oldest once the history is full.
    pub fn push(&mut self, value: f32) {
        if self.values.len() == HISTORY_LEN {
            self.values.pop_front();
        }
        self.values.push_back(value);
    }

    /// Oldest first.
    pub fn values(&self) -> impl ExactSizeIterator<Item = f32> + '_ {
        self.values.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_the_newest_readings() {
        let mut history = History::default();
        for ix in 0..HISTORY_LEN + 5 {
            history.push(ix as f32);
        }
        assert_eq!(history.values().len(), HISTORY_LEN);
        assert_eq!(history.values().next(), Some(5.));
        assert_eq!(history.values().last(), Some((HISTORY_LEN + 4) as f32));
    }
}
