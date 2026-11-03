//! Playback position in *source* time when the speed can change (phase vocoder).
//!
//! The device plays `played` output frames; one output frame is `speed` source frames, so the source
//! position is a sum over segments of constant speed. A speed change reaches the speakers only after
//! the audio already queued (ring buffer + pending samples), so the engine registers the change at the
//! output frame where it will become audible (`push_speed`). No audio, no device: unit-tested below.

#[derive(Debug, Clone)]
pub struct PositionTracker {
    rate: f64,
    /// Output-frame counter value (`played`) at which `anchor_ms` is valid.
    anchor_frames: u64,
    anchor_ms: f64,
    /// Speed in effect from the anchor on.
    speed: f32,
    /// Later speed changes: (output frame where it starts, new speed), in increasing order.
    segments: Vec<(u64, f32)>,
}

impl PositionTracker {
    pub fn new(rate: u32) -> Self {
        Self { rate: rate.max(1) as f64, anchor_frames: 0, anchor_ms: 0.0, speed: 1.0, segments: Vec::new() }
    }

    /// New track or seek: source position `ms` is playing at output frame `played` with `speed`.
    pub fn reset(&mut self, played: u64, ms: u64, speed: f32) {
        self.anchor_frames = played;
        self.anchor_ms = ms as f64;
        self.speed = speed;
        self.segments.clear();
    }

    /// The speed changes to `speed` at output frame `at_frame` (when the queued audio has been played).
    pub fn push_speed(&mut self, at_frame: u64, speed: f32) {
        // Drop a not yet reached change that this one replaces or precedes.
        while self.segments.last().is_some_and(|&(f, _)| f >= at_frame) {
            self.segments.pop();
        }
        self.segments.push((at_frame, speed));
    }

    /// Folds the changes already reached into the anchor (keeps `segments` short).
    pub fn settle(&mut self, played: u64) {
        let reached = self.segments.iter().take_while(|&&(f, _)| f <= played).count();
        for (f, s) in self.segments.drain(..reached) {
            let f = f.max(self.anchor_frames);
            self.anchor_ms += (f - self.anchor_frames) as f64 * 1000.0 / self.rate * self.speed as f64;
            self.anchor_frames = f;
            self.speed = s;
        }
    }

    /// Source position in ms at output frame `played`.
    pub fn position_ms(&self, played: u64) -> u64 {
        let mut frames = self.anchor_frames;
        let mut ms = self.anchor_ms;
        let mut speed = self.speed;
        for &(f, s) in &self.segments {
            if f > played {
                break;
            }
            let f = f.max(frames);
            ms += (f - frames) as f64 * 1000.0 / self.rate * speed as f64;
            frames = f;
            speed = s;
        }
        ms += played.saturating_sub(frames) as f64 * 1000.0 / self.rate * speed as f64;
        ms as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_speed() {
        let mut p = PositionTracker::new(1000);
        p.reset(0, 0, 2.0);
        assert_eq!(p.position_ms(1000), 2000);
        p.reset(500, 10_000, 0.5);
        assert_eq!(p.position_ms(1500), 10_500);
    }

    #[test]
    fn change_applies_only_after_queued_audio() {
        let mut p = PositionTracker::new(1000);
        p.reset(0, 0, 1.0);
        p.push_speed(400, 2.0); // 400 frames still queued at speed 1
        assert_eq!(p.position_ms(400), 400);
        assert_eq!(p.position_ms(1400), 400 + 2000);
        p.settle(1400);
        assert_eq!(p.position_ms(1400), 2400);
    }

    #[test]
    fn newer_change_replaces_unreached_one() {
        let mut p = PositionTracker::new(1000);
        p.reset(0, 0, 1.0);
        p.push_speed(400, 2.0);
        p.push_speed(300, 3.0);
        assert_eq!(p.position_ms(1300), 300 + 3000);
    }
}
