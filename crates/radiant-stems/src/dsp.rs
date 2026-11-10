//! STFT / iSTFT exactly as htdemucs does them around the network (`HTDemucs._spec` / `_ispec`, torch.stft with
//! `n_fft = 4096`, `hop = 1024`, periodic Hann window, `normalized = True`, `center = True`, reflect padding):
//!
//! forward: the segment is reflect-padded by `PAD = 1536` on the left and `PAD + FRAMES*HOP - len` on the right;
//!          frame `t` then covers `x_ext[t*HOP .. t*HOP + 4096]` (the two extra frames torch computes at each end are
//!          cut off by the model, so they are never computed here). Scale `1/sqrt(4096)`, top bin dropped.
//! inverse: add a zero bin at the top, irfft, window, overlap-add, divide by the window envelope (Hann^2 at hop N/4
//!          is exactly 1.5), scale `sqrt(4096)`, cut `PAD` samples at the start.
//!
//! Layout of the spectrogram (flat, row-major): `[4][FREQ_BINS][FRAMES]` = left re, left im, right re, right im.

use crate::demucs::*;
use realfft::num_complex::Complex32;
use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

/// Reflect padding before the first frame (`hop / 2 * 3`).
const PAD: usize = HOP / 2 * 3;
/// Length of the padded segment.
const EXT_LEN: usize = FRAMES * HOP + 2 * PAD;
/// `1 / sqrt(N_FFT)`: torch `normalized = True`.
const SCALE: f32 = 1.0 / 64.0;
/// Sum of the squared window over the overlapping frames (periodic Hann, hop = N/4).
const ENVELOPE: f32 = 1.5;

/// Index into a signal of `len` samples with reflect padding (the edge sample is not repeated).
fn reflect(i: isize, len: usize) -> usize {
    let l = len as isize;
    let mut i = i;
    if i < 0 {
        i = -i;
    }
    if i >= l {
        i = 2 * (l - 1) - i;
    }
    i.clamp(0, l - 1) as usize
}

pub struct Spectro {
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
    window: Vec<f32>,
    frame: Vec<f32>,
    bins: Vec<Complex32>,
    ext: Vec<f32>,
}

impl Spectro {
    pub fn new() -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let window = (0..N_FFT)
            .map(|n| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * n as f64 / N_FFT as f64).cos() as f32)
            .collect();
        Self {
            fwd: planner.plan_fft_forward(N_FFT),
            inv: planner.plan_fft_inverse(N_FFT),
            window,
            frame: vec![0.0; N_FFT],
            bins: vec![Complex32::new(0.0, 0.0); N_FFT / 2 + 1],
            ext: vec![0.0; EXT_LEN],
        }
    }

    /// `mix`: `[CHANNELS][SEGMENT_SAMPLES]` -> `spec`: `[SPEC_CHANNELS][FREQ_BINS][FRAMES]`.
    pub fn forward(&mut self, mix: &[f32], spec: &mut [f32]) {
        assert_eq!(mix.len(), MIX_LEN);
        assert_eq!(spec.len(), SPEC_LEN);
        for c in 0..CHANNELS {
            let x = &mix[c * SEGMENT_SAMPLES..(c + 1) * SEGMENT_SAMPLES];
            for (i, v) in self.ext.iter_mut().enumerate() {
                *v = x[reflect(i as isize - PAD as isize, SEGMENT_SAMPLES)];
            }
            for t in 0..FRAMES {
                let part = &self.ext[t * HOP..t * HOP + N_FFT];
                for n in 0..N_FFT {
                    self.frame[n] = part[n] * self.window[n] * SCALE;
                }
                self.fwd.process(&mut self.frame, &mut self.bins).expect("FFT buffer sizes are fixed");
                let re = (2 * c) * FREQ_BINS * FRAMES + t;
                let im = (2 * c + 1) * FREQ_BINS * FRAMES + t;
                for f in 0..FREQ_BINS {
                    spec[re + f * FRAMES] = self.bins[f].re;
                    spec[im + f * FRAMES] = self.bins[f].im;
                }
            }
        }
    }

    /// Inverse transform of ONE stem's spectrogram `[SPEC_CHANNELS][FREQ_BINS][FRAMES]`; the result
    /// (`[CHANNELS][SEGMENT_SAMPLES]`) is ADDED to `out` (that is how the time and the frequency branch are summed).
    pub fn inverse_add(&mut self, spec: &[f32], out: &mut [f32]) {
        assert_eq!(spec.len(), SPEC_LEN);
        assert_eq!(out.len(), MIX_LEN);
        for c in 0..CHANNELS {
            self.ext.iter_mut().for_each(|v| *v = 0.0);
            let re = (2 * c) * FREQ_BINS * FRAMES;
            let im = (2 * c + 1) * FREQ_BINS * FRAMES;
            for t in 0..FRAMES {
                for f in 0..FREQ_BINS {
                    self.bins[f] = Complex32::new(spec[re + f * FRAMES + t], spec[im + f * FRAMES + t]);
                }
                self.bins[FREQ_BINS] = Complex32::new(0.0, 0.0); // the dropped top bin
                self.bins[0].im = 0.0;
                self.inv.process(&mut self.bins, &mut self.frame).expect("FFT buffer sizes are fixed");
                let dst = &mut self.ext[t * HOP..t * HOP + N_FFT];
                for n in 0..N_FFT {
                    // The unnormalized irfft gives N * x; `SCALE` turns it into torch's `sqrt(N)` rescale.
                    dst[n] += self.frame[n] * self.window[n] * SCALE / ENVELOPE;
                }
            }
            let dst = &mut out[c * SEGMENT_SAMPLES..(c + 1) * SEGMENT_SAMPLES];
            for (d, s) in dst.iter_mut().zip(&self.ext[PAD..PAD + SEGMENT_SAMPLES]) {
                *d += *s;
            }
        }
    }
}

impl Default for Spectro {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflect_does_not_repeat_the_edge() {
        assert_eq!(reflect(-1, 10), 1);
        assert_eq!(reflect(-3, 10), 3);
        assert_eq!(reflect(10, 10), 8);
        assert_eq!(reflect(12, 10), 6);
        assert_eq!(reflect(4, 10), 4);
    }

    #[test]
    fn forward_then_inverse_restores_the_signal() {
        let mut mix = vec![0.0_f32; MIX_LEN];
        for n in 0..SEGMENT_SAMPLES {
            let t = n as f32 / SAMPLE_RATE as f32;
            mix[n] = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin();
            mix[SEGMENT_SAMPLES + n] = 0.3 * (2.0 * std::f32::consts::PI * 1250.0 * t).sin();
        }
        let mut sp = Spectro::new();
        let mut spec = vec![0.0_f32; SPEC_LEN];
        sp.forward(&mix, &mut spec);
        let mut back = vec![0.0_f32; MIX_LEN];
        sp.inverse_add(&spec, &mut back);
        // Away from both ends (the frames the model cuts off are missing there) the signal is restored.
        for c in 0..CHANNELS {
            for n in 2 * HOP..SEGMENT_SAMPLES - 2 * HOP {
                let (a, b) = (mix[c * SEGMENT_SAMPLES + n], back[c * SEGMENT_SAMPLES + n]);
                assert!((a - b).abs() < 1e-3, "channel {c} sample {n}: {a} vs {b}");
            }
        }
    }

    #[test]
    fn inverse_adds_instead_of_replacing() {
        let mut sp = Spectro::new();
        let spec = vec![0.0_f32; SPEC_LEN];
        let mut out = vec![0.25_f32; MIX_LEN];
        sp.inverse_add(&spec, &mut out);
        assert!(out.iter().all(|v| (*v - 0.25).abs() < 1e-9));
    }
}
