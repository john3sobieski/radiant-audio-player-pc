//! radiant-audio: playback engine.
//!
//! DONE: `Transport` = queue/index/repeat logic with no audio (testable on its own).
//! TODO (README section 6, real-time rules):
//!   1. decoder thread (symphonia) -> DSP (radiant-dsp) -> lock-free ring buffer
//!   2. cpal output stream; the callback only copies from the ring buffer (no alloc, no locks, no I/O)
//!   3. seek, 300 ms fade-in / 250 ms fade-out on play/pause, gapless-friendly track switching
//!   4. shuffle (field exists, logic does not), device change handling, underrun -> silence
//!   5. measure CPU idle / paused / playing and report it
//! Opus caveat: symphonia has no Opus decoder, check README section 2.

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
