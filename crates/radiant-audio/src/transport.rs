//! Queue / repeat logic with no audio (testable on its own).
//! TODO: shuffle (field exists, logic does not).

use std::path::{Path, PathBuf};

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

#[derive(Debug, Default)]
pub struct Transport {
    pub queue: Vec<PathBuf>,
    pub index: Option<usize>,
    pub state: PlayState,
    pub repeat: RepeatMode,
    /// TODO: not used yet.
    pub shuffle: bool,
}

impl Transport {
    pub fn set_queue(&mut self, queue: Vec<PathBuf>, start: usize) {
        self.index = if start < queue.len() { Some(start) } else { None };
        self.queue = queue;
    }

    pub fn current(&self) -> Option<&Path> {
        self.queue.get(self.index?).map(PathBuf::as_path)
    }

    /// `auto_end` = the track ended by itself (repeat-one then replays it);
    /// a user pressing "next" passes `false`.
    pub fn next(&mut self, auto_end: bool) -> Option<&Path> {
        let i = self.index?;
        let new = if auto_end && self.repeat == RepeatMode::One {
            i
        } else if i + 1 < self.queue.len() {
            i + 1
        } else if self.repeat == RepeatMode::All {
            0
        } else {
            self.state = PlayState::Stopped;
            return None;
        };
        self.index = Some(new);
        self.queue.get(new).map(PathBuf::as_path)
    }

    pub fn previous(&mut self) -> Option<&Path> {
        let i = self.index?;
        let new = if i > 0 {
            i - 1
        } else if self.repeat == RepeatMode::All {
            self.queue.len().saturating_sub(1)
        } else {
            0
        };
        self.index = Some(new);
        self.queue.get(new).map(PathBuf::as_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q() -> Transport {
        let mut t = Transport::default();
        t.set_queue(vec!["a".into(), "b".into(), "c".into()], 0);
        t
    }

    #[test]
    fn next_stops_at_end_without_repeat() {
        let mut t = q();
        t.state = PlayState::Playing;
        t.next(false);
        t.next(false);
        assert!(t.next(true).is_none());
        assert_eq!(t.state, PlayState::Stopped);
    }

    #[test]
    fn repeat_all_wraps_and_repeat_one_replays() {
        let mut t = q();
        t.repeat = RepeatMode::All;
        t.index = Some(2);
        assert_eq!(t.next(true), Some(Path::new("a")));
        t.repeat = RepeatMode::One;
        assert_eq!(t.next(true), Some(Path::new("a")));
        assert_eq!(t.next(false), Some(Path::new("b")));
    }

    #[test]
    fn previous_clamps_at_start() {
        let mut t = q();
        assert_eq!(t.previous(), Some(Path::new("a")));
    }
}
