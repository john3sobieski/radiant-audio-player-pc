//! Estimate of the time left in a long job (stem separation takes minutes, so the interface shows "about 1:20 left").
//!
//! The estimate is the elapsed time divided by the share done, so it assumes the speed stays the same. The first
//! seconds say nothing (loading the model, decoding), therefore no number is given until a few seconds have passed
//! and a few percent are done. The estimated TOTAL time is smoothed (not the remaining time), which keeps the
//! shown number calm and counting down.

use std::time::Instant;

/// No estimate before this many seconds of work ...
const MIN_ELAPSED: f32 = 4.0;
/// ... and before this share (0..1) is done.
const MIN_PROGRESS: f32 = 0.03;
/// Weight of the newest sample in the smoothed total (0..1).
const SMOOTHING: f32 = 0.25;

/// Raw estimate of the total duration, or `None` while it is too early to say.
pub fn estimate_total(elapsed_secs: f32, progress: f32) -> Option<f32> {
    if elapsed_secs < MIN_ELAPSED || !(MIN_PROGRESS..=1.0).contains(&progress) {
        return None;
    }
    Some(elapsed_secs / progress)
}

pub struct Eta {
    start: Instant,
    total: Option<f32>,
}

impl Default for Eta {
    fn default() -> Self {
        Self::new()
    }
}

impl Eta {
    /// Starts counting now.
    pub fn new() -> Self {
        Self { start: Instant::now(), total: None }
    }

    /// Feed the current progress (0..1); returns the seconds left, or `None` while it is too early.
    pub fn update(&mut self, progress: f32) -> Option<u64> {
        let elapsed = self.start.elapsed().as_secs_f32();
        self.update_at(elapsed, progress)
    }

    /// Same with an explicit elapsed time (for tests).
    pub fn update_at(&mut self, elapsed_secs: f32, progress: f32) -> Option<u64> {
        let sample = estimate_total(elapsed_secs, progress)?;
        let total = match self.total {
            Some(prev) => prev + SMOOTHING * (sample - prev),
            None => sample,
        };
        self.total = Some(total);
        Some((total - elapsed_secs).max(0.0).ceil() as u64)
    }
}

/// 83 -> "1:23", 3725 -> "1:02:05".
pub fn format_secs(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn too_early_gives_nothing() {
        assert_eq!(estimate_total(1.0, 0.5), None); // too little time
        assert_eq!(estimate_total(10.0, 0.01), None); // too little work
        assert_eq!(estimate_total(10.0, 1.5), None); // nonsense
    }

    #[test]
    fn linear_progress_gives_the_right_rest() {
        let mut eta = Eta::new();
        // 10 s for 25 % -> 40 s in total -> 30 s left.
        assert_eq!(eta.update_at(10.0, 0.25), Some(30));
        // Still the same speed: 20 s for 50 % -> 20 s left.
        assert_eq!(eta.update_at(20.0, 0.5), Some(20));
        // Done.
        assert_eq!(eta.update_at(40.0, 1.0), Some(0));
    }

    #[test]
    fn a_slow_start_is_smoothed_not_jumped() {
        let mut eta = Eta::new();
        assert_eq!(eta.update_at(10.0, 0.1), Some(90)); // total 100
        // The next sample says total 60; the shown total moves only a quarter of the way: 90.
        let left = eta.update_at(30.0, 0.5).unwrap();
        assert!(left > 30 && left < 70, "{left}");
    }

    #[test]
    fn time_text() {
        assert_eq!(format_secs(0), "0:00");
        assert_eq!(format_secs(83), "1:23");
        assert_eq!(format_secs(3725), "1:02:05");
    }
}
