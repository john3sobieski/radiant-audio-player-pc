//! Spectrum analyzer for the player rays: mono samples in, smoothed log-spaced bands out.
//!
//! Pure DSP, no I/O and no allocation after `new` / `set_rate`. The UI feeds it the audio that was just
//! played (see `radiant-audio/src/spectrum.rs`) and calls `update(dt)` once per frame.
//!
//! Pipeline: last `FFT_N` samples -> Hann window -> radix-2 FFT -> magnitude -> `BANDS` log-spaced
//! bands (40 Hz .. 16 kHz) -> dB with a slight tilt (music is bass-heavy) -> 0..1 -> light smoothing
//! across neighbours -> fast attack / slow release in time -> falling peak markers.

use std::f32::consts::PI;

pub const FFT_N: usize = 2048;
pub const BANDS: usize = 60;

const F_LO: f32 = 40.0;
const F_HI: f32 = 16_000.0;
/// dB range mapped to 0..1 (after the tilt).
const DB_FLOOR: f32 = -78.0;
const DB_CEIL: f32 = -20.0;
/// Pink-ish compensation: dB added per octave above `TILT_REF_HZ`.
const TILT_DB_PER_OCT: f32 = 3.0;
const TILT_REF_HZ: f32 = 250.0;
/// Time constants (seconds) of the smoothing.
const ATTACK_S: f32 = 0.030;
const RELEASE_S: f32 = 0.160;
/// Peak marker: hold time, then it falls with this acceleration (units of full scale per s^2).
const PEAK_HOLD_S: f32 = 0.35;
const PEAK_GRAVITY: f32 = 2.2;
/// Bands that make up the "bass" value (about 40 .. 160 Hz).
const BASS_BANDS: usize = 9;

#[derive(Clone, Debug)]
pub struct SpectrumFrame {
    /// Smoothed level per band, 0..1, low frequencies first.
    pub bands: [f32; BANDS],
    /// Falling peak markers, always >= `bands`.
    pub peaks: [f32; BANDS],
    /// Smoothed low-frequency energy, 0..1 (drives the pulse of the glow).
    pub bass: f32,
    /// Smoothed overall level, 0..1.
    pub level: f32,
}

impl Default for SpectrumFrame {
    fn default() -> Self {
        SpectrumFrame { bands: [0.0; BANDS], peaks: [0.0; BANDS], bass: 0.0, level: 0.0 }
    }
}

pub struct SpectrumAnalyzer {
    rate: u32,
    hist: Vec<f32>,
    window: Vec<f32>,
    cos_t: Vec<f32>,
    sin_t: Vec<f32>,
    re: Vec<f32>,
    im: Vec<f32>,
    mag: Vec<f32>,
    /// Per band: first bin, last bin (inclusive) and the center position in bins.
    ranges: [(usize, usize, f32); BANDS],
    /// Per band: tilt in dB.
    tilt: [f32; BANDS],
    raw: [f32; BANDS],
    peak_hold: [f32; BANDS],
    peak_vel: [f32; BANDS],
    frame: SpectrumFrame,
}

impl SpectrumAnalyzer {
    pub fn new(rate: u32) -> Self {
        let window: Vec<f32> =
            (0..FFT_N).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FFT_N as f32).cos()).collect();
        let half = FFT_N / 2;
        let cos_t: Vec<f32> = (0..half).map(|k| (2.0 * PI * k as f32 / FFT_N as f32).cos()).collect();
        let sin_t: Vec<f32> = (0..half).map(|k| -(2.0 * PI * k as f32 / FFT_N as f32).sin()).collect();
        let mut a = SpectrumAnalyzer {
            rate: 0,
            hist: vec![0.0; FFT_N],
            window,
            cos_t,
            sin_t,
            re: vec![0.0; FFT_N],
            im: vec![0.0; FFT_N],
            mag: vec![0.0; half + 1],
            ranges: [(0, 0, 0.0); BANDS],
            tilt: [0.0; BANDS],
            raw: [0.0; BANDS],
            peak_hold: [0.0; BANDS],
            peak_vel: [0.0; BANDS],
            frame: SpectrumFrame::default(),
        };
        a.set_rate(rate);
        a
    }

    /// Recomputes the band layout for a new sample rate (call when the output device changes).
    pub fn set_rate(&mut self, rate: u32) {
        let rate = rate.max(8000);
        if rate == self.rate {
            return;
        }
        self.rate = rate;
        let bin_hz = rate as f32 / FFT_N as f32;
        let f_hi = F_HI.min(rate as f32 * 0.45);
        let max_bin = FFT_N / 2;
        let edge = |i: usize| F_LO * (f_hi / F_LO).powf(i as f32 / BANDS as f32);
        for b in 0..BANDS {
            let (lo, hi) = (edge(b), edge(b + 1));
            let first = ((lo / bin_hz).ceil() as usize).min(max_bin);
            let last = ((hi / bin_hz).floor() as usize).min(max_bin);
            let center = (lo * hi).sqrt() / bin_hz;
            self.ranges[b] = (first, last.max(first), center);
            self.tilt[b] = TILT_DB_PER_OCT * ((lo * hi).sqrt() / TILT_REF_HZ).log2();
        }
    }

    pub fn reset(&mut self) {
        self.hist.fill(0.0);
        self.raw = [0.0; BANDS];
        self.peak_hold = [0.0; BANDS];
        self.peak_vel = [0.0; BANDS];
        self.frame = SpectrumFrame::default();
    }

    /// Appends mono samples (oldest first).
    pub fn push(&mut self, samples: &[f32]) {
        let n = samples.len();
        if n == 0 {
            return;
        }
        if n >= FFT_N {
            self.hist.copy_from_slice(&samples[n - FFT_N..]);
        } else {
            self.hist.copy_within(n.., 0);
            self.hist[FFT_N - n..].copy_from_slice(samples);
        }
    }

    /// Appends `n` samples of silence (nothing is playing).
    pub fn push_silence(&mut self, n: usize) {
        let n = n.min(FFT_N);
        if n == 0 {
            return;
        }
        self.hist.copy_within(n.., 0);
        self.hist[FFT_N - n..].fill(0.0);
    }

    /// Analyzes the current window and advances the smoothing by `dt` seconds.
    pub fn update(&mut self, dt: f32) -> &SpectrumFrame {
        let dt = dt.clamp(0.001, 0.25);
        self.analyze();

        // Light smoothing across neighbours: the silhouette of the rays stays round, not spiky.
        let mut target = [0.0_f32; BANDS];
        for b in 0..BANDS {
            let l = self.raw[b.saturating_sub(1)];
            let r = self.raw[(b + 1).min(BANDS - 1)];
            target[b] = 0.25 * l + 0.5 * self.raw[b] + 0.25 * r;
        }

        let up = 1.0 - (-dt / ATTACK_S).exp();
        let down = 1.0 - (-dt / RELEASE_S).exp();
        for b in 0..BANDS {
            let cur = self.frame.bands[b];
            let k = if target[b] > cur { up } else { down };
            let v = cur + (target[b] - cur) * k;
            self.frame.bands[b] = v;

            if v >= self.frame.peaks[b] {
                self.frame.peaks[b] = v;
                self.peak_hold[b] = PEAK_HOLD_S;
                self.peak_vel[b] = 0.0;
            } else if self.peak_hold[b] > 0.0 {
                self.peak_hold[b] -= dt;
            } else {
                self.peak_vel[b] += PEAK_GRAVITY * dt;
                self.frame.peaks[b] = (self.frame.peaks[b] - self.peak_vel[b] * dt).max(v);
            }
        }

        let bass_target = self.raw[..BASS_BANDS].iter().sum::<f32>() / BASS_BANDS as f32;
        let level_target = self.raw.iter().sum::<f32>() / BANDS as f32;
        let bk = if bass_target > self.frame.bass { 1.0 - (-dt / 0.02).exp() } else { 1.0 - (-dt / 0.22).exp() };
        self.frame.bass += (bass_target - self.frame.bass) * bk;
        let lk = if level_target > self.frame.level { up } else { down };
        self.frame.level += (level_target - self.frame.level) * lk;
        &self.frame
    }

    fn analyze(&mut self) {
        for i in 0..FFT_N {
            self.re[i] = self.hist[i] * self.window[i];
            self.im[i] = 0.0;
        }
        self.fft();
        // Amplitude of a full-scale sine = 1.0 (Hann window sums to N/2).
        let norm = 2.0 / (FFT_N as f32 * 0.5);
        for k in 0..=FFT_N / 2 {
            self.mag[k] = (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt() * norm;
        }
        for b in 0..BANDS {
            let (first, last, center) = self.ranges[b];
            let mut m = if last > first {
                self.mag[first..=last].iter().copied().fold(0.0_f32, f32::max)
            } else {
                0.0
            };
            // Narrow low bands: interpolate between the two bins around the center.
            let c0 = center.floor() as usize;
            let c1 = (c0 + 1).min(FFT_N / 2);
            let fr = center - c0 as f32;
            let interp = self.mag[c0.min(FFT_N / 2)] * (1.0 - fr) + self.mag[c1] * fr;
            m = m.max(interp);
            let db = 20.0 * m.max(1e-9).log10() + self.tilt[b];
            self.raw[b] = ((db - DB_FLOOR) / (DB_CEIL - DB_FLOOR)).clamp(0.0, 1.0);
        }
    }

    /// In-place iterative radix-2 FFT of (`re`, `im`).
    fn fft(&mut self) {
        let n = FFT_N;
        let mut j = 0;
        for i in 1..n {
            let mut bit = n >> 1;
            while j & bit != 0 {
                j ^= bit;
                bit >>= 1;
            }
            j ^= bit;
            if i < j {
                self.re.swap(i, j);
                self.im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let step = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..len / 2 {
                    let (wr, wi) = (self.cos_t[k * step], self.sin_t[k * step]);
                    let (a, b) = (start + k, start + k + len / 2);
                    let tr = self.re[b] * wr - self.im[b] * wi;
                    let ti = self.re[b] * wi + self.im[b] * wr;
                    self.re[b] = self.re[a] - tr;
                    self.im[b] = self.im[a] - ti;
                    self.re[a] += tr;
                    self.im[a] += ti;
                }
            }
            len <<= 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, n: usize, amp: f32) -> Vec<f32> {
        (0..n).map(|i| amp * (2.0 * PI * freq * i as f32 / rate as f32).sin()).collect()
    }

    fn loudest(a: &SpectrumAnalyzer) -> usize {
        (0..BANDS).max_by(|&x, &y| a.frame.bands[x].total_cmp(&a.frame.bands[y])).unwrap()
    }

    fn band_center(rate: u32, b: usize) -> f32 {
        let f_hi = F_HI.min(rate as f32 * 0.45);
        let e = |i: usize| F_LO * (f_hi / F_LO).powf(i as f32 / BANDS as f32);
        (e(b) * e(b + 1)).sqrt()
    }

    #[test]
    fn sine_lights_the_right_band() {
        for &(rate, freq) in &[(48_000, 1000.0), (44_100, 3000.0), (48_000, 100.0), (48_000, 9000.0)] {
            let mut a = SpectrumAnalyzer::new(rate);
            a.push(&sine(freq, rate, FFT_N, 0.5));
            for _ in 0..30 {
                a.update(0.016);
            }
            let b = loudest(&a);
            let c = band_center(rate, b);
            assert!((c / freq).log2().abs() < 0.2, "{freq} Hz landed in the band at {c} Hz");
            assert!(a.frame.bands[b] > 0.4, "weak band: {}", a.frame.bands[b]);
        }
    }

    #[test]
    fn silence_is_dark_and_peaks_fall() {
        let mut a = SpectrumAnalyzer::new(48_000);
        a.push(&sine(1000.0, 48_000, FFT_N, 0.8));
        for _ in 0..30 {
            a.update(0.016);
        }
        let b = loudest(&a);
        let lit = a.frame.peaks[b];
        assert!(lit > 0.5);
        for _ in 0..120 {
            a.push_silence(768);
            a.update(0.016);
        }
        assert!(a.frame.bands.iter().all(|v| *v < 0.01));
        assert!(a.frame.peaks[b] < lit * 0.2, "peak did not fall: {}", a.frame.peaks[b]);
        assert!(a.frame.level < 0.01 && a.frame.bass < 0.01);
    }

    #[test]
    fn peaks_never_below_bands_and_values_in_range() {
        let mut a = SpectrumAnalyzer::new(44_100);
        let mut seed = 12345_u32;
        for _ in 0..50 {
            let noise: Vec<f32> = (0..700)
                .map(|_| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    (seed >> 8) as f32 / (1 << 24) as f32 * 2.0 - 1.0
                })
                .collect();
            a.push(&noise);
            let f = a.update(0.016).clone();
            for b in 0..BANDS {
                assert!((0.0..=1.0).contains(&f.bands[b]));
                assert!(f.peaks[b] >= f.bands[b] - 1e-6);
            }
        }
    }

    #[test]
    fn bass_follows_low_notes_only() {
        let mut a = SpectrumAnalyzer::new(48_000);
        a.push(&sine(60.0, 48_000, FFT_N, 0.7));
        for _ in 0..40 {
            a.update(0.016);
        }
        let low = a.frame.bass;
        a.reset();
        a.push(&sine(8000.0, 48_000, FFT_N, 0.7));
        for _ in 0..40 {
            a.update(0.016);
        }
        assert!(low > 0.15 && a.frame.bass < 0.02, "low {low}, high {}", a.frame.bass);
    }
}
