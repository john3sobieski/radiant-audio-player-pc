//! radiant-dsp: effects chain and time stretch. Pure functions over sample buffers, no audio device.
//!
//! DONE (README 5.1, written but not yet compiled by the owner's CI): biquads (RBJ cookbook),
//!       12-band EQ with the Q formula, bass/treble shelves, subsonic Butterworth pair, preamp,
//!       normalization, soft limiter, mono downmix, zero-cost bypass, unit tests.
//! TODO (README 5.2): phase vocoder with identity phase locking (port TimeStretch.kt).
//! Rules: no allocation in `process`, settings replaced whole (version counter), f64 filter state.
//! Read Android `AudioFx.kt` before porting. Keep the numbers from the README.

use serde::{Deserialize, Serialize};

pub mod presets;
pub mod spectrum;
pub mod stretch;

/// Centre frequencies of the 12 EQ bands, Hz (README 5.1).
pub const EQ_BANDS_HZ: [f64; 12] = [
    32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 3500.0, 6000.0, 9000.0, 12500.0, 16000.0,
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
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

const MAX_CHANNELS: usize = 8;
/// 12 EQ bands + bass + treble + 2 subsonic sections.
const MAX_FILTERS: usize = 16;
const SLOT_BASS: usize = 12;
const SLOT_TREBLE: usize = 13;
const SLOT_SUB1: usize = 14;
const SLOT_SUB2: usize = 15;

const NORM_TARGET_RMS: f64 = 0.16;
const NORM_ENV_ATTACK_S: f64 = 0.25;
const NORM_ENV_RELEASE_S: f64 = 0.8;
const NORM_GATE_POWER: f64 = 1e-6;
const NORM_MIN_GAIN: f64 = 0.25;
const NORM_MAX_GAIN: f64 = 2.8;
const NORM_GAIN_ATTACK_S: f64 = 0.3;
const NORM_GAIN_RELEASE_S: f64 = 2.0;
const LIMIT_KNEE: f64 = 0.9;

/// One biquad section, normalized (a0 = 1). RBJ Audio EQ Cookbook formulas.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Coeffs {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

impl Coeffs {
    fn normalized(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Self { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0 }
    }

    fn peaking(f: f64, fs: f64, gain_db: f64, q: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * f / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::normalized(
            1.0 + alpha * a,
            -2.0 * cos,
            1.0 - alpha * a,
            1.0 + alpha / a,
            -2.0 * cos,
            1.0 - alpha / a,
        )
    }

    fn shelf_alpha(a: f64, sin: f64, slope: f64) -> f64 {
        sin / 2.0 * ((a + 1.0 / a) * (1.0 / slope - 1.0) + 2.0).sqrt()
    }

    fn low_shelf(f: f64, fs: f64, gain_db: f64, slope: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * f / fs;
        let (sin, cos) = w0.sin_cos();
        let t = 2.0 * a.sqrt() * Self::shelf_alpha(a, sin, slope);
        Self::normalized(
            a * ((a + 1.0) - (a - 1.0) * cos + t),
            2.0 * a * ((a - 1.0) - (a + 1.0) * cos),
            a * ((a + 1.0) - (a - 1.0) * cos - t),
            (a + 1.0) + (a - 1.0) * cos + t,
            -2.0 * ((a - 1.0) + (a + 1.0) * cos),
            (a + 1.0) + (a - 1.0) * cos - t,
        )
    }

    fn high_shelf(f: f64, fs: f64, gain_db: f64, slope: f64) -> Self {
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * f / fs;
        let (sin, cos) = w0.sin_cos();
        let t = 2.0 * a.sqrt() * Self::shelf_alpha(a, sin, slope);
        Self::normalized(
            a * ((a + 1.0) + (a - 1.0) * cos + t),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * cos),
            a * ((a + 1.0) + (a - 1.0) * cos - t),
            (a + 1.0) - (a - 1.0) * cos + t,
            2.0 * ((a - 1.0) - (a + 1.0) * cos),
            (a + 1.0) - (a - 1.0) * cos - t,
        )
    }

    fn high_pass(f: f64, fs: f64, q: f64) -> Self {
        let w0 = 2.0 * std::f64::consts::PI * f / fs;
        let (sin, cos) = w0.sin_cos();
        let alpha = sin / (2.0 * q);
        Self::normalized(
            (1.0 + cos) / 2.0,
            -(1.0 + cos),
            (1.0 + cos) / 2.0,
            1.0 + alpha,
            -2.0 * cos,
            1.0 - alpha,
        )
    }

    /// Magnitude of the transfer function at `f` Hz, in dB (used by tests).
    #[cfg(test)]
    fn magnitude_db(&self, f: f64, fs: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * f / fs;
        let (s1, c1) = w.sin_cos();
        let (s2, c2) = (2.0 * w).sin_cos();
        let nr = self.b0 + self.b1 * c1 + self.b2 * c2;
        let ni = -(self.b1 * s1 + self.b2 * s2);
        let dr = 1.0 + self.a1 * c1 + self.a2 * c2;
        let di = -(self.a1 * s1 + self.a2 * s2);
        10.0 * ((nr * nr + ni * ni) / (dr * dr + di * di)).log10()
    }
}

/// Q of EQ band `i` so that neighbouring bands overlap evenly (README 5.1).
fn band_q(i: usize) -> f64 {
    let f = EQ_BANDS_HZ[i];
    let lo = if i == 0 { f / 2.0 } else { EQ_BANDS_HZ[i - 1] };
    let hi = if i == EQ_BANDS_HZ.len() - 1 { f * 2.0 } else { EQ_BANDS_HZ[i + 1] };
    let octaves = (hi / lo).ln() / std::f64::consts::LN_2 / 2.0;
    1.0 / (2.0 * (std::f64::consts::LN_2 / 2.0 * octaves).sinh())
}

fn smoothing_coef(sample_rate: f64, seconds: f64) -> f64 {
    1.0 - (-1.0 / (sample_rate * seconds)).exp()
}

fn soft_limit(x: f64) -> f64 {
    let a = x.abs();
    if a <= LIMIT_KNEE {
        x
    } else {
        let over = LIMIT_KNEE + (1.0 - LIMIT_KNEE) * ((a - LIMIT_KNEE) / (1.0 - LIMIT_KNEE)).tanh();
        over.copysign(x)
    }
}

/// Filter state of one section for one channel (direct form II transposed).
#[derive(Clone, Copy, Default)]
struct State {
    z1: f64,
    z2: f64,
}

pub struct AudioFxProcessor {
    settings: AudioFxSettings,
    dirty: bool,
    sample_rate: u32,
    coeffs: [Option<Coeffs>; MAX_FILTERS],
    /// Previous activity of each slot, to zero the state when a filter is switched off.
    state: [[State; MAX_FILTERS]; MAX_CHANNELS],
    preamp: f64,
    // Normalization.
    env_attack: f64,
    env_release: f64,
    gain_attack: f64,
    gain_release: f64,
    env: f64,
    gain: f64,
    // Cached "is anything active" for the current settings.
    any_filter: bool,
}

impl AudioFxProcessor {
    pub fn new(settings: AudioFxSettings) -> Self {
        Self {
            settings,
            dirty: true,
            sample_rate: 0,
            coeffs: [None; MAX_FILTERS],
            state: [[State::default(); MAX_FILTERS]; MAX_CHANNELS],
            preamp: 1.0,
            env_attack: 0.0,
            env_release: 0.0,
            gain_attack: 0.0,
            gain_release: 0.0,
            env: NORM_TARGET_RMS * NORM_TARGET_RMS,
            gain: 1.0,
            any_filter: false,
        }
    }

    /// Replaces the settings whole; coefficients are rebuilt lazily on the next `process`.
    pub fn set_settings(&mut self, settings: AudioFxSettings) {
        if settings != self.settings {
            self.settings = settings;
            self.dirty = true;
        }
    }

    fn rebuild(&mut self, sample_rate: u32) {
        let fs = sample_rate as f64;
        let s = &self.settings;
        let mut new: [Option<Coeffs>; MAX_FILTERS] = [None; MAX_FILTERS];
        let mut max_boost = 0.0_f64;

        for (i, &f) in EQ_BANDS_HZ.iter().enumerate() {
            let g = s.eq_gains_db[i].clamp(-12.0, 12.0) as f64;
            if g.abs() < 0.05 || f >= 0.45 * fs {
                continue;
            }
            new[i] = Some(Coeffs::peaking(f, fs, g, band_q(i)));
            max_boost = max_boost.max(g);
        }
        let bass = s.bass_db.clamp(0.0, 15.0) as f64;
        if bass >= 0.05 {
            new[SLOT_BASS] = Some(Coeffs::low_shelf(110.0, fs, bass, 1.0));
            max_boost = max_boost.max(bass);
        }
        let treble = s.treble_db.clamp(0.0, 15.0) as f64;
        if treble >= 0.05 && 6000.0 < 0.45 * fs {
            new[SLOT_TREBLE] = Some(Coeffs::high_shelf(6000.0, fs, treble, 1.0));
            max_boost = max_boost.max(treble);
        }
        let sub = s.subsonic_percent.clamp(0.0, 100.0) as f64;
        if sub > 0.5 {
            let cutoff = 12.0 + (sub / 100.0) * (40.0 - 12.0);
            new[SLOT_SUB1] = Some(Coeffs::high_pass(cutoff, fs, 0.5412));
            new[SLOT_SUB2] = Some(Coeffs::high_pass(cutoff, fs, 1.3066));
        }

        // A filter that was switched off or changed from "off" gets a clean state (no click).
        for slot in 0..MAX_FILTERS {
            if self.coeffs[slot].is_none() || new[slot].is_none() {
                for ch in self.state.iter_mut() {
                    ch[slot] = State::default();
                }
            }
        }
        self.coeffs = new;
        self.any_filter = new.iter().any(|c| c.is_some());
        self.preamp = 10f64.powf(-0.5 * max_boost / 20.0);

        self.env_attack = smoothing_coef(fs, NORM_ENV_ATTACK_S);
        self.env_release = smoothing_coef(fs, NORM_ENV_RELEASE_S);
        self.gain_attack = smoothing_coef(fs, NORM_GAIN_ATTACK_S);
        self.gain_release = smoothing_coef(fs, NORM_GAIN_RELEASE_S);
        self.sample_rate = sample_rate;
        self.dirty = false;
    }

    /// Processes interleaved `f32` samples in place. No allocation.
    pub fn process(&mut self, samples: &mut [f32], channels: usize, sample_rate: u32) {
        if channels == 0 || sample_rate == 0 {
            return;
        }
        if self.dirty || self.sample_rate != sample_rate {
            self.rebuild(sample_rate);
        }
        let mono = self.settings.mono && channels >= 2;
        let normalize = self.settings.normalize;
        if !mono && !normalize && !self.any_filter {
            return; // bypass: audio untouched
        }
        let limit = normalize || self.any_filter;
        let fx_channels = channels.min(MAX_CHANNELS);

        for frame in samples.chunks_exact_mut(channels) {
            if mono {
                let m = (frame[0] + frame[1]) * 0.5;
                frame[0] = m;
                frame[1] = m;
            }

            let mut norm_gain = 1.0;
            if normalize {
                let mut p = 0.0;
                for &x in frame.iter() {
                    p += (x as f64) * (x as f64);
                }
                p /= channels as f64;
                let c = if p > self.env { self.env_attack } else { self.env_release };
                self.env += c * (p - self.env);
                if self.env > NORM_GATE_POWER {
                    let desired =
                        (NORM_TARGET_RMS / self.env.sqrt()).clamp(NORM_MIN_GAIN, NORM_MAX_GAIN);
                    let c = if desired < self.gain { self.gain_attack } else { self.gain_release };
                    self.gain += c * (desired - self.gain);
                }
                norm_gain = self.gain;
            }

            for (ch, x) in frame.iter_mut().enumerate() {
                let mut v = (*x as f64) * norm_gain;
                if ch < fx_channels && self.any_filter {
                    v *= self.preamp;
                    let st = &mut self.state[ch];
                    for (slot, c) in self.coeffs.iter().enumerate() {
                        if let Some(c) = c {
                            let s = &mut st[slot];
                            let y = c.b0 * v + s.z1;
                            s.z1 = c.b1 * v - c.a1 * y + s.z2;
                            s.z2 = c.b2 * v - c.a2 * y;
                            v = y;
                        }
                    }
                }
                if limit {
                    v = soft_limit(v);
                }
                *x = v as f32;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: u32 = 44_100;

    fn sine_stereo(freq: f64, amp: f64, frames: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(frames * 2);
        for n in 0..frames {
            let s = (amp * (2.0 * std::f64::consts::PI * freq * n as f64 / FS as f64).sin()) as f32;
            v.push(s);
            v.push(s);
        }
        v
    }

    fn rms_left(buf: &[f32], skip_frames: usize) -> f64 {
        let xs: Vec<f64> = buf.chunks_exact(2).skip(skip_frames).map(|f| f[0] as f64).collect();
        (xs.iter().map(|x| x * x).sum::<f64>() / xs.len() as f64).sqrt()
    }

    #[test]
    fn neutral_settings_do_not_touch_audio() {
        let mut fx = AudioFxProcessor::new(AudioFxSettings::default());
        let mut buf = [0.1_f32, -0.2, 0.3, -0.4];
        let copy = buf;
        fx.process(&mut buf, 2, 44_100);
        assert_eq!(buf, copy);
    }

    #[test]
    fn tiny_gains_stay_bypassed() {
        let mut s = AudioFxSettings::default();
        s.eq_gains_db[3] = 0.02;
        let mut fx = AudioFxProcessor::new(s);
        let mut buf = sine_stereo(1000.0, 0.5, 256);
        let copy = buf.clone();
        fx.process(&mut buf, 2, FS);
        assert_eq!(buf, copy);
    }

    #[test]
    fn peaking_gain_at_centre_matches_setting() {
        for i in 0..12 {
            for g in [-12.0, -6.0, 6.0, 12.0] {
                let c = Coeffs::peaking(EQ_BANDS_HZ[i], 48_000.0, g, band_q(i));
                let db = c.magnitude_db(EQ_BANDS_HZ[i], 48_000.0);
                assert!((db - g).abs() < 0.01, "band {i} gain {g}: got {db}");
            }
        }
    }

    #[test]
    fn peaking_is_flat_far_away() {
        let c = Coeffs::peaking(1000.0, 48_000.0, 12.0, band_q(5));
        assert!(c.magnitude_db(20.0, 48_000.0).abs() < 0.5);
        assert!(c.magnitude_db(20_000.0, 48_000.0).abs() < 1.0);
    }

    #[test]
    fn shelves_reach_their_gain() {
        let low = Coeffs::low_shelf(110.0, 48_000.0, 15.0, 1.0);
        assert!((low.magnitude_db(10.0, 48_000.0) - 15.0).abs() < 0.2);
        assert!(low.magnitude_db(10_000.0, 48_000.0).abs() < 0.2);
        let high = Coeffs::high_shelf(6000.0, 48_000.0, 15.0, 1.0);
        assert!((high.magnitude_db(20_000.0, 48_000.0) - 15.0).abs() < 0.5);
        assert!(high.magnitude_db(100.0, 48_000.0).abs() < 0.2);
    }

    #[test]
    fn subsonic_is_24db_per_octave_butterworth() {
        let fs = 48_000.0;
        let f = 40.0;
        let a = Coeffs::high_pass(f, fs, 0.5412);
        let b = Coeffs::high_pass(f, fs, 1.3066);
        let at = |x: f64| a.magnitude_db(x, fs) + b.magnitude_db(x, fs);
        assert!((at(f) + 3.01).abs() < 0.1, "-3 dB at cutoff, got {}", at(f));
        assert!(at(5_000.0).abs() < 0.01);
        // One octave below the cutoff a 4th-order Butterworth is about -24 dB.
        assert!((at(f / 2.0) + 24.1).abs() < 1.0, "got {}", at(f / 2.0));
    }

    #[test]
    fn eq_boost_changes_level_by_expected_amount() {
        let mut s = AudioFxSettings::default();
        s.eq_gains_db[5] = 12.0; // 1 kHz
        let mut fx = AudioFxProcessor::new(s);
        let mut buf = sine_stereo(1000.0, 0.1, FS as usize);
        let before = rms_left(&buf, 0);
        fx.process(&mut buf, 2, FS);
        let after = rms_left(&buf, 4000);
        let db = 20.0 * (after / before).log10();
        // +12 dB band, minus the preamp (half of the boost = -6 dB) -> about +6 dB.
        assert!((db - 6.0).abs() < 0.3, "got {db}");
    }

    #[test]
    fn mono_downmix_makes_channels_equal() {
        let mut s = AudioFxSettings::default();
        s.mono = true;
        let mut fx = AudioFxProcessor::new(s);
        let mut buf = [1.0_f32, 0.0, 0.5, -0.5];
        fx.process(&mut buf, 2, FS);
        assert_eq!(buf, [0.5, 0.5, 0.0, 0.0]);
    }

    #[test]
    fn limiter_never_exceeds_one() {
        let mut s = AudioFxSettings::default();
        s.bass_db = 15.0;
        let mut fx = AudioFxProcessor::new(s);
        let mut buf = sine_stereo(60.0, 1.0, FS as usize);
        fx.process(&mut buf, 2, FS);
        assert!(buf.iter().all(|x| x.abs() <= 1.0));
        assert!(soft_limit(0.5) == 0.5);
        assert!(soft_limit(5.0) <= 1.0 && soft_limit(-5.0) >= -1.0);
    }

    #[test]
    fn normalization_converges_to_target_level() {
        let mut s = AudioFxSettings::default();
        s.normalize = true;
        let mut fx = AudioFxProcessor::new(s);
        // Quiet sine, RMS 0.08 -> needs gain 2.0 (inside the 0.25..2.8 range).
        // The asymmetric power follower (fast attack) tracks sine peaks a bit above the mean
        // power, so the settled level lands slightly below the 0.16 target, never above it.
        let amp = 0.08 * 2f64.sqrt();
        let mut buf = sine_stereo(440.0, amp, FS as usize * 15);
        fx.process(&mut buf, 2, FS);
        let tail = rms_left(&buf[buf.len() - FS as usize * 2..], 0);
        assert!(tail > 0.12 && tail < 0.165, "got {tail}");
    }

    #[test]
    fn normalization_gain_is_frozen_on_silence() {
        let mut s = AudioFxSettings::default();
        s.normalize = true;
        let mut fx = AudioFxProcessor::new(s);
        let mut loud = sine_stereo(440.0, 0.3, FS as usize * 5);
        fx.process(&mut loud, 2, FS);
        let g = fx.gain;
        let _ = g;
        // The envelope needs ~9 s to decay below the gate; after that the gain must not move.
        let mut silence = vec![0.0_f32; FS as usize * 20];
        fx.process(&mut silence, 2, FS);
        let frozen = fx.gain;
        let mut more = vec![0.0_f32; FS as usize * 10];
        fx.process(&mut more, 2, FS);
        assert_eq!(fx.gain, frozen);
    }

    #[test]
    fn switching_filter_off_resets_its_state() {
        let mut s = AudioFxSettings::default();
        s.bass_db = 10.0;
        let mut fx = AudioFxProcessor::new(s);
        let mut buf = sine_stereo(60.0, 0.5, 4096);
        fx.process(&mut buf, 2, FS);
        fx.set_settings(AudioFxSettings::default());
        let mut buf = sine_stereo(60.0, 0.5, 64);
        let copy = buf.clone();
        fx.process(&mut buf, 2, FS);
        assert_eq!(buf, copy);
        assert!(fx.state[0].iter().all(|s| s.z1 == 0.0 && s.z2 == 0.0));
    }

    #[test]
    fn sample_rate_change_rebuilds_coefficients() {
        let mut s = AudioFxSettings::default();
        s.treble_db = 12.0;
        let mut fx = AudioFxProcessor::new(s);
        let mut a = sine_stereo(8000.0, 0.1, 2048);
        fx.process(&mut a, 2, 44_100);
        let c1 = fx.coeffs[SLOT_TREBLE];
        let mut b = sine_stereo(8000.0, 0.1, 2048);
        fx.process(&mut b, 2, 96_000);
        assert_ne!(c1, fx.coeffs[SLOT_TREBLE]);
    }
}
