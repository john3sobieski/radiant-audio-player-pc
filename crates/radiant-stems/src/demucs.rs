//! Demucs (htdemucs, "Hybrid Transformer Demucs v4", 4 stems) as an ONNX network: the contract the rest of the
//! pipeline relies on, the slicing of a track into segments, and the [`Backend`] trait that the ONNX Runtime
//! wrapper will implement.
//!
//! The ONNX exports of htdemucs cannot contain the STFT / iSTFT (ONNX has no complex FFT), so the program does them
//! itself around every network call:
//!
//! ```text
//! waveform segment [2, SEGMENT_SAMPLES] --STFT--> spec [4, FREQ_BINS, FRAMES]   (L.re, L.im, R.re, R.im)
//! network(mix, spec) -> wave [4 stems, 2, SEGMENT_SAMPLES]        (time branch)
//!                       spec_out [4 stems, 4, FREQ_BINS, FRAMES]  (frequency branch) --iSTFT--> [4, 2, SEGMENT_SAMPLES]
//! stem = wave + iSTFT(spec_out)
//! ```
//!
//! Different community exports use different tensor NAMES (`spec` / `mag`, `wave` / `time`, `spec_out` / `freq`), so
//! [`check_io`] recognizes the tensors by SHAPE, and the pinned model in `store.rs` decides which export is used.

use crate::StemError;

/// Sample rate of the network (the track is resampled to it).
pub const SAMPLE_RATE: u32 = 44_100;
pub const CHANNELS: usize = 2;
/// Number of stems of the 4-stem model, in network order: drums, bass, other, vocals.
pub const STEMS: usize = 4;
/// One network call: 7.8 s of audio (the length htdemucs was trained on).
pub const SEGMENT_SAMPLES: usize = 343_980;
pub const N_FFT: usize = 4096;
pub const HOP: usize = 1024;
/// The network sees 2048 of the 2049 bins (the top bin is dropped).
pub const FREQ_BINS: usize = N_FFT / 2;
/// STFT frames of one segment: `ceil(SEGMENT_SAMPLES / HOP)`.
pub const FRAMES: usize = 336;
/// The spectrogram has four channels: left real, left imaginary, right real, right imaginary.
pub const SPEC_CHANNELS: usize = 4;
/// Share of a segment that overlaps the next one (the Demucs default).
pub const OVERLAP: f32 = 0.25;

/// Floats in the `mix` input / one stem of the `wave` output.
pub const MIX_LEN: usize = CHANNELS * SEGMENT_SAMPLES;
/// Floats in the `spec` input / one stem of the `spec_out` output.
pub const SPEC_LEN: usize = SPEC_CHANNELS * FREQ_BINS * FRAMES;

/// What a network call returns (flat, row-major, batch dimension removed).
pub struct SegmentOut {
    /// `[STEMS, CHANNELS, SEGMENT_SAMPLES]`: the time branch.
    pub wave: Vec<f32>,
    /// `[STEMS, SPEC_CHANNELS, FREQ_BINS, FRAMES]`: the frequency branch (still to be inverse-transformed).
    pub spec_out: Vec<f32>,
}

/// The network runner. The ONNX Runtime wrapper (`ort`, CPU first) implements this; everything above it (STFT,
/// overlap-add, WAV writing) works on this trait only, so it can be tested with a fake.
pub trait Backend: Send {
    /// `mix`: `[CHANNELS, SEGMENT_SAMPLES]`, `spec`: `[SPEC_CHANNELS, FREQ_BINS, FRAMES]`, both flat and row-major.
    fn run(&mut self, mix: &[f32], spec: &[f32]) -> Result<SegmentOut, StemError>;
}

/// Name and shape of a tensor as the ONNX session reports it (`-1` = dynamic dimension).
#[derive(Clone, Debug)]
pub struct TensorInfo {
    pub name: String,
    pub dims: Vec<i64>,
}

/// Which of the session's tensors is which (indices into the lists given to [`check_io`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IoMap {
    pub mix_in: usize,
    pub spec_in: usize,
    pub wave_out: usize,
    pub spec_out: usize,
}

fn dim_ok(actual: i64, wanted: usize) -> bool {
    actual < 0 || actual as usize == wanted
}

/// Checks that a loaded model has the htdemucs 4-stem contract and finds its tensors by shape:
/// inputs `[1, 2, SEGMENT]` and `[1, 4, 2048, FRAMES]`, outputs `[1, 4, 2, SEGMENT]` and `[1, 4, 4, 2048, FRAMES]`.
/// Dynamic dimensions are accepted; fixed ones must match. The error text says what is wrong (6-stem model, other
/// segment length, ...), it is meant for the log, not for translation.
pub fn check_io(inputs: &[TensorInfo], outputs: &[TensorInfo]) -> Result<IoMap, String> {
    let find = |list: &[TensorInfo], rank: usize, pred: &dyn Fn(&[i64]) -> bool| {
        list.iter().position(|t| t.dims.len() == rank && pred(t.dims.as_slice()))
    };
    let mix_in = find(inputs, 3, &|d: &[i64]| d[1] == CHANNELS as i64 && dim_ok(d[2], SEGMENT_SAMPLES))
        .ok_or("no waveform input [1, 2, 343980]: wrong model or another segment length")?;
    let spec_in = find(inputs, 4, &|d: &[i64]| {
        d[1] == SPEC_CHANNELS as i64 && dim_ok(d[2], FREQ_BINS) && dim_ok(d[3], FRAMES)
    })
    .ok_or("no spectrogram input [1, 4, 2048, 336]")?;
    if inputs.len() != 2 {
        return Err(format!("expected 2 inputs, the model has {}", inputs.len()));
    }
    let stems = |d: &[i64]| d[1] >= 0 && d[1] as usize == STEMS;
    let wave_out = match find(outputs, 4, &|d: &[i64]| d[1] >= 0 && d[2] == CHANNELS as i64 && dim_ok(d[3], SEGMENT_SAMPLES)) {
        Some(i) if stems(outputs[i].dims.as_slice()) => i,
        Some(i) => return Err(format!("the model has {} stems, 4 are needed", outputs[i].dims[1])),
        None => return Err("no waveform output [1, 4, 2, 343980]".into()),
    };
    let spec_out = find(outputs, 5, &|d: &[i64]| {
        stems(d) && d[2] == SPEC_CHANNELS as i64 && dim_ok(d[3], FREQ_BINS) && dim_ok(d[4], FRAMES)
    })
    .ok_or("no spectrogram output [1, 4, 4, 2048, 336]")?;
    Ok(IoMap { mix_in, spec_in, wave_out, spec_out })
}

/// One network call's piece of the track: `len` real samples starting at `start` (the rest of the segment, if any,
/// is zero padding that is thrown away).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub start: usize,
    pub len: usize,
}

/// Distance between the starts of two segments.
pub fn stride() -> usize {
    ((1.0 - OVERLAP) * SEGMENT_SAMPLES as f32) as usize
}

/// Slices a track of `total` samples per channel into overlapping segments. The last segment is the first one that
/// reaches the end of the track (no extra segment that earlier ones already cover). Empty for an empty track.
pub fn plan(total: usize) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    if total == 0 {
        return chunks;
    }
    let mut start = 0;
    loop {
        chunks.push(Chunk { start, len: SEGMENT_SAMPLES.min(total - start) });
        if start + SEGMENT_SAMPLES >= total {
            return chunks;
        }
        start += stride();
    }
}

/// Triangle window for blending overlapping segments (as in Demucs `apply_model`, `transition_power = 1`):
/// rises to 1 in the middle, never reaches 0, so the sum of the windows is positive on every sample.
pub fn overlap_weights() -> Vec<f32> {
    let half = SEGMENT_SAMPLES / 2;
    let peak = half as f32;
    let mut w = Vec::with_capacity(SEGMENT_SAMPLES);
    w.extend((1..=half).map(|i| i as f32 / peak));
    w.extend((1..=SEGMENT_SAMPLES - half).rev().map(|i| i as f32 / peak));
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(name: &str, dims: &[i64]) -> TensorInfo {
        TensorInfo { name: name.into(), dims: dims.to_vec() }
    }

    #[test]
    fn frames_match_segment() {
        assert_eq!(FRAMES, (SEGMENT_SAMPLES + HOP - 1) / HOP);
        assert_eq!(FREQ_BINS, 2048);
        assert_eq!(SEGMENT_SAMPLES, (7.8 * SAMPLE_RATE as f64).round() as usize);
    }

    #[test]
    fn io_is_found_by_shape_whatever_the_names() {
        let a = check_io(
            &[t("mix", &[1, 2, 343980]), t("spec", &[1, 4, 2048, 336])],
            &[t("wave", &[1, 4, 2, 343980]), t("spec_out", &[1, 4, 4, 2048, 336])],
        );
        assert_eq!(a, Ok(IoMap { mix_in: 0, spec_in: 1, wave_out: 0, spec_out: 1 }));
        // Other names and another output order.
        let b = check_io(
            &[t("mag", &[1, 4, 2048, 336]), t("mix", &[1, 2, 343980])],
            &[t("freq", &[1, 4, 4, 2048, 336]), t("time", &[1, 4, 2, 343980])],
        );
        assert_eq!(b, Ok(IoMap { mix_in: 1, spec_in: 0, wave_out: 1, spec_out: 0 }));
        // Dynamic batch and length are accepted.
        assert!(check_io(
            &[t("a", &[-1, 2, -1]), t("b", &[-1, 4, 2048, -1])],
            &[t("c", &[-1, 4, 2, -1]), t("d", &[-1, 4, 4, 2048, -1])],
        )
        .is_ok());
    }

    #[test]
    fn wrong_models_are_rejected_with_a_reason() {
        let six = check_io(
            &[t("mix", &[1, 2, 343980]), t("spec", &[1, 4, 2048, 336])],
            &[t("wave", &[1, 6, 2, 343980]), t("spec_out", &[1, 6, 4, 2048, 336])],
        );
        assert!(six.unwrap_err().contains("6 stems"));
        let short = check_io(
            &[t("mix", &[1, 2, 171990]), t("spec", &[1, 4, 2048, 168])],
            &[t("wave", &[1, 4, 2, 171990]), t("spec_out", &[1, 4, 4, 2048, 168])],
        );
        assert!(short.is_err(), "the 3.9 s export needs a different segment size");
        assert!(check_io(&[t("x", &[1, 2, 343980])], &[]).is_err());
    }

    #[test]
    fn plan_covers_the_track() {
        assert!(plan(0).is_empty());
        assert_eq!(plan(1000), vec![Chunk { start: 0, len: 1000 }]);
        assert_eq!(plan(SEGMENT_SAMPLES), vec![Chunk { start: 0, len: SEGMENT_SAMPLES }]);
        for total in [SEGMENT_SAMPLES + 1, 3 * SEGMENT_SAMPLES, 44_100 * 240] {
            let chunks = plan(total);
            let last = chunks.last().unwrap();
            assert_eq!(last.start + last.len, total);
            for pair in chunks.windows(2) {
                assert_eq!(pair[1].start - pair[0].start, stride());
                assert!(pair[1].start < pair[0].start + pair[0].len, "segments must overlap");
            }
        }
        // 4 minutes: 240 s * 44100 / 257985 -> 41 segments.
        assert_eq!(plan(44_100 * 240).len(), 41);
    }

    #[test]
    fn weights_are_a_positive_triangle() {
        let w = overlap_weights();
        assert_eq!(w.len(), SEGMENT_SAMPLES);
        assert!(w.iter().all(|x| *x > 0.0 && *x <= 1.0));
        assert!((w[SEGMENT_SAMPLES / 2 - 1] - 1.0).abs() < 1e-6);
        assert!(w[0] < 1e-4 && w[SEGMENT_SAMPLES - 1] < 1e-4);
    }
}
