//! The separation itself: a whole track (44.1 kHz stereo) -> four stems, segment by segment through a [`Backend`].
//! Works on the trait only, so it is tested with a fake network (no model, no ONNX Runtime needed).
//!
//! As in the Demucs command line: the track is normalized once (mono mix: subtract the mean, divide by the standard
//! deviation) and the result is scaled back. Segments overlap by 25 %, are blended with the triangle window of
//! `apply_model` and divided by the sum of the windows. Per segment the network gets the waveform and its STFT; a stem
//! is `wave + iSTFT(spec_out)`.

use crate::demucs::{self, Backend, CHANNELS, MIX_LEN, SEGMENT_SAMPLES, SPEC_LEN, STEMS};
use crate::dsp::Spectro;
use crate::stem::{Stem, StemSet};
use crate::StemError;
use std::sync::atomic::{AtomicBool, Ordering};

/// Same value as Demucs (`ref.std() + 1e-8`).
const STD_EPS: f32 = 1e-8;

/// The four separated stems, planar float, at the sample rate of the network.
pub struct Stems {
    frames: usize,
    /// `[STEMS][CHANNELS][frames]`.
    data: Vec<f32>,
}

impl Stems {
    pub fn frames(&self) -> usize {
        self.frames
    }

    pub fn channel(&self, stem: Stem, ch: usize) -> &[f32] {
        let at = (stem.index() * CHANNELS + ch) * self.frames;
        &self.data[at..at + self.frames]
    }

    /// Mixes the stems of `set` into `out` (interleaved stereo) for the frames starting at `start`.
    pub fn mix_block(&self, set: StemSet, start: usize, out: &mut [f32]) {
        out.iter_mut().for_each(|v| *v = 0.0);
        let n = out.len() / 2;
        for stem in set.members() {
            let (l, r) = (self.channel(stem, 0), self.channel(stem, 1));
            for i in 0..n {
                out[2 * i] += l[start + i];
                out[2 * i + 1] += r[start + i];
            }
        }
    }
}

/// Separates `samples` (interleaved stereo). `on_segment(done, total)` is called after every network call.
pub fn separate(
    samples: &[f32],
    backend: &mut dyn Backend,
    cancel: &AtomicBool,
    on_segment: &mut dyn FnMut(usize, usize),
) -> Result<Stems, StemError> {
    let n = samples.len() / 2;
    let mut left: Vec<f32> = samples.iter().step_by(2).copied().collect();
    let mut right: Vec<f32> = samples.iter().skip(1).step_by(2).copied().collect();
    // Normalization by the mono mix of the whole track.
    let mean = (left.iter().zip(&right).map(|(l, r)| (*l as f64 + *r as f64) / 2.0).sum::<f64>() / n.max(1) as f64) as f32;
    let var = left
        .iter()
        .zip(&right)
        .map(|(l, r)| {
            let d = (*l as f64 + *r as f64) / 2.0 - mean as f64;
            d * d
        })
        .sum::<f64>()
        / n.max(1) as f64;
    let std = var.sqrt() as f32 + STD_EPS;
    for v in left.iter_mut().chain(right.iter_mut()) {
        *v = (*v - mean) / std;
    }

    let plan = demucs::plan(n);
    let weights = demucs::overlap_weights();
    let mut spectro = Spectro::new();
    let mut acc = vec![0.0_f32; STEMS * CHANNELS * n];
    let mut weight_sum = vec![0.0_f32; n];
    let mut mix = vec![0.0_f32; MIX_LEN];
    let mut spec = vec![0.0_f32; SPEC_LEN];
    let mut stem_wave = vec![0.0_f32; MIX_LEN];

    for (i, chunk) in plan.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(StemError::Cancelled);
        }
        mix.iter_mut().for_each(|v| *v = 0.0); // the tail of the last segment is silence
        mix[..chunk.len].copy_from_slice(&left[chunk.start..chunk.start + chunk.len]);
        mix[SEGMENT_SAMPLES..SEGMENT_SAMPLES + chunk.len].copy_from_slice(&right[chunk.start..chunk.start + chunk.len]);
        spectro.forward(&mix, &mut spec);
        let out = backend.run(&mix, &spec)?;
        for s in 0..STEMS {
            stem_wave.copy_from_slice(&out.wave[s * MIX_LEN..(s + 1) * MIX_LEN]);
            spectro.inverse_add(&out.spec_out[s * SPEC_LEN..(s + 1) * SPEC_LEN], &mut stem_wave);
            for c in 0..CHANNELS {
                let dst = &mut acc[(s * CHANNELS + c) * n + chunk.start..][..chunk.len];
                let src = &stem_wave[c * SEGMENT_SAMPLES..][..chunk.len];
                for ((d, v), w) in dst.iter_mut().zip(src).zip(&weights) {
                    *d += v * w;
                }
            }
        }
        for (d, w) in weight_sum[chunk.start..chunk.start + chunk.len].iter_mut().zip(&weights) {
            *d += w;
        }
        on_segment(i + 1, plan.len());
    }

    for plane in acc.chunks_exact_mut(n.max(1)) {
        for (v, w) in plane.iter_mut().zip(&weight_sum) {
            *v = *v / w * std + mean;
        }
    }
    Ok(Stems { frames: n, data: acc })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::demucs::SegmentOut;

    /// A fake network: stem 0 (drums) is the whole input, the other stems are silent, the frequency branch is zero.
    struct Passthrough {
        calls: usize,
    }

    impl Backend for Passthrough {
        fn run(&mut self, mix: &[f32], _spec: &[f32]) -> Result<SegmentOut, StemError> {
            self.calls += 1;
            let mut wave = vec![0.0; STEMS * MIX_LEN];
            wave[..MIX_LEN].copy_from_slice(mix);
            Ok(SegmentOut { wave, spec_out: vec![0.0; STEMS * SPEC_LEN] })
        }
    }

    fn track(frames: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(frames * 2);
        for i in 0..frames {
            let t = i as f32 / 44_100.0;
            v.push(0.4 * (2.0 * std::f32::consts::PI * 220.0 * t).sin() + 0.1);
            v.push(0.3 * (2.0 * std::f32::consts::PI * 330.0 * t).sin() - 0.05);
        }
        v
    }

    #[test]
    fn overlap_add_and_normalization_restore_the_input() {
        // Two and a bit segments: three network calls, a short last segment.
        let frames = SEGMENT_SAMPLES * 2 + 5_000;
        let input = track(frames);
        let mut backend = Passthrough { calls: 0 };
        let mut seen = Vec::new();
        let stems = separate(&input, &mut backend, &AtomicBool::new(false), &mut |d, t| seen.push((d, t))).unwrap();
        assert_eq!(backend.calls, demucs::plan(frames).len());
        assert_eq!(seen.last().copied(), Some((backend.calls, backend.calls)));
        assert_eq!(stems.frames(), frames);
        for i in (0..frames).step_by(997) {
            assert!((stems.channel(Stem::Drums, 0)[i] - input[2 * i]).abs() < 2e-3, "left at {i}");
            assert!((stems.channel(Stem::Drums, 1)[i] - input[2 * i + 1]).abs() < 2e-3, "right at {i}");
            assert!(stems.channel(Stem::Vocals, 0)[i].abs() < 0.2 + 1e-3); // silence + the track mean
        }
        // Mixing stems into a block (interleaved): drums alone, then drums + bass.
        let mut out = vec![0.0; 2 * 100];
        stems.mix_block(StemSet::EMPTY.with(Stem::Drums), 500, &mut out);
        assert!((out[0] - stems.channel(Stem::Drums, 0)[500]).abs() < 1e-6);
        stems.mix_block(StemSet::EMPTY.with(Stem::Drums).with(Stem::Bass), 500, &mut out);
        let both = stems.channel(Stem::Drums, 1)[501] + stems.channel(Stem::Bass, 1)[501];
        assert!((out[3] - both).abs() < 1e-6);
    }

    #[test]
    fn cancel_stops_before_the_first_call() {
        let input = track(1000);
        let mut backend = Passthrough { calls: 0 };
        let r = separate(&input, &mut backend, &AtomicBool::new(true), &mut |_, _| {});
        assert!(matches!(r, Err(StemError::Cancelled)));
        assert_eq!(backend.calls, 0);
    }

    #[test]
    fn silence_does_not_blow_up() {
        let input = vec![0.0_f32; 2 * 2_000];
        let stems = separate(&input, &mut Passthrough { calls: 0 }, &AtomicBool::new(false), &mut |_, _| {}).unwrap();
        assert!(stems.channel(Stem::Drums, 0).iter().all(|v| v.is_finite() && v.abs() < 1e-3));
    }
}
