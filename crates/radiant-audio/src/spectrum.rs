//! Live spectrum tap. The cpal callback copies a mono mix of what it plays into a lock-free ring
//! (only while the UI asks for it); the UI thread drains that ring once per frame and runs the
//! analyzer from `radiant-dsp`. The audio thread never takes a lock and never waits for the UI.

use radiant_dsp::spectrum::{SpectrumAnalyzer, SpectrumFrame};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Mono samples kept for the analyzer (about 340 ms at 48 kHz; the UI drains it every ~16 ms).
const TAP_CAPACITY: usize = 16_384;

struct Inner {
    cons: Option<rtrb::Consumer<f32>>,
    analyzer: SpectrumAnalyzer,
    scratch: Vec<f32>,
    /// `enabled` was off at the last poll: stale samples must be dropped first.
    was_off: bool,
}

/// Cloneable handle: the engine attaches a new ring whenever the output stream is (re)opened,
/// the UI calls `poll` every frame.
#[derive(Clone)]
pub struct SpectrumTap {
    inner: Arc<Mutex<Inner>>,
    enabled: Arc<AtomicBool>,
}

impl SpectrumTap {
    pub(crate) fn new() -> Self {
        SpectrumTap {
            inner: Arc::new(Mutex::new(Inner {
                cons: None,
                analyzer: SpectrumAnalyzer::new(48_000),
                scratch: Vec::with_capacity(TAP_CAPACITY),
                was_off: true,
            })),
            enabled: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Engine side: a new ring for a new stream. Returns the producer for the callback and the
    /// flag that tells it whether to feed the ring.
    pub(crate) fn attach(&self, rate: u32) -> (rtrb::Producer<f32>, Arc<AtomicBool>) {
        let (prod, cons) = rtrb::RingBuffer::<f32>::new(TAP_CAPACITY);
        if let Ok(mut g) = self.inner.lock() {
            g.cons = Some(cons);
            g.analyzer.set_rate(rate);
            g.analyzer.reset();
        }
        (prod, self.enabled.clone())
    }

    /// UI side: call once per frame while the rays are visible. `dt` = seconds since the last call.
    /// Returns a copy of the smoothed frame (bands low -> high, 0..1).
    pub fn poll(&self, dt: f32) -> SpectrumFrame {
        self.enabled.store(true, Ordering::Relaxed);
        let Ok(mut g) = self.inner.lock() else { return SpectrumFrame::default() };
        let inner = &mut *g;
        let Some(cons) = inner.cons.as_mut() else { return inner.analyzer.update(dt).clone() };
        inner.scratch.clear();
        while let Ok(s) = cons.pop() {
            inner.scratch.push(s);
        }
        if inner.was_off {
            // The ring filled up while nobody looked: only the newest part is of interest.
            inner.was_off = false;
            let keep = 2048.min(inner.scratch.len());
            let skip = inner.scratch.len() - keep;
            inner.scratch.drain(..skip);
        }
        if inner.scratch.is_empty() {
            // Paused or idle: the audio callback writes nothing, so let the picture fade out.
            inner.analyzer.push_silence((dt * 48_000.0) as usize);
        } else {
            inner.analyzer.push(&inner.scratch);
        }
        inner.analyzer.update(dt).clone()
    }

    /// UI side: the rays are hidden, stop feeding the ring (the callback then does no extra work).
    pub fn pause(&self) {
        self.enabled.store(false, Ordering::Relaxed);
        if let Ok(mut g) = self.inner.lock() {
            g.was_off = true;
        }
    }
}
