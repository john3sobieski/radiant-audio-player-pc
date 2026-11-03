//! Queue / repeat / shuffle logic with no audio (testable on its own).
//!
//! The playing order is a list of queue indices (`order`): identity when shuffle is off, a random
//! permutation when it is on (current track first). `pos` is the position inside `order`.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RepeatMode {
    #[default]
    Off,
    All,
    One,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayState {
    #[default]
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug)]
pub struct Transport {
    pub queue: Vec<PathBuf>,
    pub state: PlayState,
    pub repeat: RepeatMode,
    shuffle: bool,
    order: Vec<usize>,
    pos: usize,
    rng: u64,
}

impl Default for Transport {
    fn default() -> Self {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        Self {
            queue: Vec::new(),
            state: PlayState::Stopped,
            repeat: RepeatMode::Off,
            shuffle: false,
            order: Vec::new(),
            pos: 0,
            rng: seed | 1, // xorshift must not start at 0
        }
    }
}

impl Transport {
    fn rand(&mut self) -> u64 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng = x;
        x
    }

    /// Rebuilds `order` so that `current` (a queue index) is the playing track.
    fn build_order(&mut self, current: usize) {
        let n = self.queue.len();
        if self.shuffle {
            let mut rest: Vec<usize> = (0..n).filter(|&i| i != current).collect();
            for i in (1..rest.len()).rev() {
                let j = (self.rand() % (i as u64 + 1)) as usize;
                rest.swap(i, j);
            }
            self.order = std::iter::once(current).chain(rest).collect();
            self.pos = 0;
        } else {
            self.order = (0..n).collect();
            self.pos = current;
        }
    }

    pub fn set_queue(&mut self, queue: Vec<PathBuf>, start: usize) {
        self.queue = queue;
        if start < self.queue.len() {
            self.build_order(start);
        } else {
            self.order.clear();
            self.pos = 0;
        }
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// Turning shuffle on/off keeps the current track playing.
    pub fn set_shuffle(&mut self, on: bool) {
        if on == self.shuffle {
            return;
        }
        self.shuffle = on;
        if let Some(cur) = self.order.get(self.pos).copied() {
            self.build_order(cur);
        }
    }

    pub fn current(&self) -> Option<&Path> {
        let i = *self.order.get(self.pos)?;
        self.queue.get(i).map(PathBuf::as_path)
    }

    /// `auto_end` = the track ended by itself (repeat-one then replays it);
    /// a user pressing "next" passes `false`.
    pub fn next(&mut self, auto_end: bool) -> Option<&Path> {
        if self.order.is_empty() {
            return None;
        }
        if auto_end && self.repeat == RepeatMode::One {
            // same track
        } else if self.pos + 1 < self.order.len() {
            self.pos += 1;
        } else if self.repeat == RepeatMode::All {
            // New round: reshuffle (or restart the list).
            let cur = self.order[self.pos];
            let first = if self.shuffle { cur } else { 0 };
            self.build_order(first);
            if !self.shuffle {
                self.pos = 0;
            }
        } else {
            self.state = PlayState::Stopped;
            return None;
        }
        self.current()
    }

    pub fn previous(&mut self) -> Option<&Path> {
        if self.order.is_empty() {
            return None;
        }
        if self.pos > 0 {
            self.pos -= 1;
        } else if self.repeat == RepeatMode::All {
            self.pos = self.order.len() - 1;
        }
        self.current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(n: usize, start: usize) -> Transport {
        let mut t = Transport::default();
        t.set_queue((0..n).map(|i| PathBuf::from(i.to_string())).collect(), start);
        t
    }

    #[test]
    fn next_stops_at_end_without_repeat() {
        let mut t = q(3, 0);
        t.state = PlayState::Playing;
        assert_eq!(t.next(false), Some(Path::new("1")));
        assert_eq!(t.next(false), Some(Path::new("2")));
        assert!(t.next(true).is_none());
        assert_eq!(t.state, PlayState::Stopped);
    }

    #[test]
    fn repeat_all_wraps_and_repeat_one_replays() {
        let mut t = q(3, 2);
        t.repeat = RepeatMode::All;
        assert_eq!(t.next(true), Some(Path::new("0")));
        t.repeat = RepeatMode::One;
        assert_eq!(t.next(true), Some(Path::new("0")));
        assert_eq!(t.next(false), Some(Path::new("1")));
    }

    #[test]
    fn previous_clamps_at_start() {
        let mut t = q(3, 0);
        assert_eq!(t.previous(), Some(Path::new("0")));
    }

    #[test]
    fn shuffle_plays_every_track_once_starting_with_current() {
        let mut t = Transport::default();
        t.set_shuffle(true);
        t.set_queue((0..5).map(|i| PathBuf::from(i.to_string())).collect(), 2);
        let mut seen = vec![t.current().unwrap().to_path_buf()];
        for _ in 0..4 {
            seen.push(t.next(false).unwrap().to_path_buf());
        }
        assert_eq!(seen[0], PathBuf::from("2"));
        seen.sort();
        let all: Vec<PathBuf> = (0..5).map(|i| PathBuf::from(i.to_string())).collect();
        assert_eq!(seen, all);
        assert!(t.next(false).is_none());
    }

    #[test]
    fn toggling_shuffle_keeps_current_track() {
        let mut t = q(6, 3);
        t.set_shuffle(true);
        assert_eq!(t.current(), Some(Path::new("3")));
        t.set_shuffle(false);
        assert_eq!(t.current(), Some(Path::new("3")));
        assert_eq!(t.next(false), Some(Path::new("4")));
    }
}
