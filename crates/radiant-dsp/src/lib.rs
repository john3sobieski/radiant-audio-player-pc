//! radiant-dsp: effects chain and time stretch. Pure functions over sample buffers, no audio device.
//!
//! DONE: settings struct, band table, zero-cost bypass skeleton.
//! TODO (README 5.1): biquads (RBJ cookbook), 12-band EQ with the Q formula, bass/treble shelves,
//!       subsonic Butterworth pair, preamp, normalization, soft limiter, mono downmix.
//! TODO (README 5.2): phase vocoder with identity phase locking (port TimeStretch.kt).
//! Rules: no allocation in `process`, settings replaced whole (version counter), f64 filter state.
//! Read Android `AudioFx.kt` before porting. Keep the numbers from the README.

/// Centre frequencies of the 12 EQ bands, Hz (README 5.1).
pub const EQ_BANDS_HZ: [f64; 12] = [
    32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 3500.0, 6000.0, 9000.0, 12500.0, 16000.0,
];

#[derive(Debug, Clone, PartialEq)]
pub struct AudioFxSettings {
    pub mono: bool,
    pub normalize: bool,
    /// EQ gains in dB, range ±12.
    pub eq_gains_db: [f32; 12],
    /// Bass shelf gain in dB, 0..15.
    pub bass_db: f32,
    /// Treble shelf gain in dB, 0..15.
    pub treble_db: f32,
    /// Subsonic filter intensity, 0..100 %.
    pub subsonic_percent: f32,
}

impl Default for AudioFxSettings {
    fn default() -> Self {
        Self {
            mono: false,
            normalize: false,
            eq_gains_db: [0.0; 12],
            bass_db: 0.0,
            treble_db: 0.0,
            subsonic_percent: 0.0,
        }
    }
}

impl AudioFxSettings {
    /// True when nothing is active, so `process` must leave audio untouched (zero-cost bypass).
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }
}

pub struct AudioFxProcessor {
    settings: AudioFxSettings,
    // TODO: filter coefficients + per-channel state, rebuilt when settings change.
}

impl AudioFxProcessor {
    pub fn new(settings: AudioFxSettings) -> Self {
        Self { settings }
    }

    pub fn set_settings(&mut self, settings: AudioFxSettings) {
        self.settings = settings;
    }

    /// Processes interleaved `f32` samples in place.
    pub fn process(&mut self, _samples: &mut [f32], _channels: usize, _sample_rate: u32) {
        if self.settings.is_neutral() {
            return; // bypass: audio untouched
        }
        // TODO: implement the chain from README 5.1.
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_settings_do_not_touch_audio() {
        let mut fx = AudioFxProcessor::new(AudioFxSettings::default());
        let mut buf = [0.1_f32, -0.2, 0.3, -0.4];
        let copy = buf;
        fx.process(&mut buf, 2, 44_100);
        assert_eq!(buf, copy);
    }
}
