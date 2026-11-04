//! Thin adapter over `rubato` (version 5): an asynchronous windowed-sinc resampler with a fixed ratio.
//! This is the ONLY file that touches the rubato API, so a future rubato update changes only this file.
//! `convert.rs` owns all the buffering logic and is unit-tested with a fake core.
//!
//! Quality / cost: sinc length 128, Blackman-Harris^2 window, cubic interpolation between 128 oversampled
//! filters (rubato's recommended high-quality mode). Stereo costs about 6 dot products of 128 taps per output frame.

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Async, FixedAsync, Indexing, Resampler, SincInterpolationParameters, WindowFunction};

/// Filter length in taps (rounded up to a multiple of 8 by rubato).
const SINC_LEN: usize = 128;

pub struct SincCore {
    rs: Async<f32>,
    channels: usize,
    /// Output scratch buffer, `output_frames_max() * channels` samples, allocated once.
    out: Vec<f32>,
}

impl SincCore {
    /// `chunk` = number of input frames per call (fixed-input mode).
    pub fn new(src_rate: u32, dst_rate: u32, channels: usize, chunk: usize) -> Result<Self, String> {
        let ratio = dst_rate as f64 / src_rate as f64;
        let params = SincInterpolationParameters::new(SINC_LEN, WindowFunction::BlackmanHarris2);
        // The ratio never changes, so no headroom for ratio adjustments (max relative ratio 1.0).
        let rs = Async::<f32>::new_sinc(ratio, 1.0, &params, chunk, channels, FixedAsync::Input)
            .map_err(|e| e.to_string())?;
        let out = vec![0.0; rs.output_frames_max() * channels];
        Ok(Self { rs, channels, out })
    }

    /// Number of input frames the next call to `process` needs.
    pub fn input_frames_next(&self) -> usize {
        self.rs.input_frames_next()
    }

    /// Frames of leading silence in the output caused by the filter delay (to be trimmed by the caller).
    pub fn delay(&self) -> usize {
        self.rs.output_delay()
    }

    /// Back to the initial state (after a seek).
    pub fn reset(&mut self) {
        self.rs.reset();
    }

    /// Resamples one chunk. `input` holds at least `input_frames_next()` frames (interleaved); with
    /// `partial = Some(n)` only the first `n` frames are real and the rest is treated as silence.
    /// Appends the produced frames (interleaved) to `out` and returns how many input frames were used.
    pub fn process(&mut self, input: &[f32], partial: Option<usize>, out: &mut Vec<f32>) -> Result<usize, String> {
        let ch = self.channels;
        let need = self.rs.input_frames_next();
        if input.len() < need * ch {
            return Err("resampler input too short".into());
        }
        let max_out = self.out.len() / ch;
        let inp = InterleavedSlice::new(&input[..need * ch], ch, need).map_err(|_| "input buffer size".to_string())?;
        let mut outp =
            InterleavedSlice::new_mut(&mut self.out[..], ch, max_out).map_err(|_| "output buffer size".to_string())?;
        let indexing = Indexing { input_offset: 0, output_offset: 0, partial_len: partial, active_channels_mask: None };
        let (used, produced) = self
            .rs
            .process_into_buffer(&inp, &mut outp, Some(&indexing))
            .map_err(|e| e.to_string())?;
        out.extend_from_slice(&self.out[..produced * ch]);
        Ok(used)
    }
}
