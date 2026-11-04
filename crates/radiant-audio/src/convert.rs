//! Channel remap + sample-rate conversion (source format -> device format).
//!
//! Resampling uses `rubato` (windowed sinc, see `sinc_core.rs`). All the buffering around it lives here, written
//! against the small `Core` trait so it can be unit-tested with a fake core:
//!  - input is collected until a full chunk is available, the filter delay is trimmed from the start,
//!  - `finish` flushes the tail at the end of a track and cuts the output to the exact expected length,
//!  - `reset` (after a seek) restarts the filter without rebuilding its tables.
//! If rubato can not be created for some odd rate pair, a plain linear interpolator is used instead
//! (never fail playback because of the resampler).

use crate::sinc_core::SincCore;

/// Input frames per resampler call (about 23 ms at 44.1 kHz).
const CHUNK_FRAMES: usize = 1024;

/// What the streaming wrapper needs from a resampler (implemented by `SincCore`, and by a fake in tests).
trait Core {
    fn input_frames_next(&self) -> usize;
    /// Frames of leading silence in the output.
    fn delay(&self) -> usize;
    fn reset(&mut self);
    /// Consumes `input_frames_next()` frames (or `partial` of them, the rest is silence), appends output to `out`.
    /// Returns the number of input frames used.
    fn process(&mut self, input: &[f32], partial: Option<usize>, out: &mut Vec<f32>) -> Result<usize, String>;
}

impl Core for SincCore {
    fn input_frames_next(&self) -> usize {
        SincCore::input_frames_next(self)
    }
    fn delay(&self) -> usize {
        SincCore::delay(self)
    }
    fn reset(&mut self) {
        SincCore::reset(self)
    }
    fn process(&mut self, input: &[f32], partial: Option<usize>, out: &mut Vec<f32>) -> Result<usize, String> {
        SincCore::process(self, input, partial, out)
    }
}

/// Buffering around a chunked resampler core: whole chunks in, exact-length stream out.
struct Streaming<C: Core> {
    core: C,
    ch: usize,
    /// Output frames per input frame.
    ratio: f64,
    /// Input frames not yet consumed by the core (interleaved).
    inbuf: Vec<f32>,
    /// Output of the last core call, before delay trimming.
    tmp: Vec<f32>,
    /// Zero-padded last chunk (finish only).
    scratch: Vec<f32>,
    /// Frames of filter delay still to be dropped from the output.
    to_skip: usize,
    in_total: u64,
    out_total: u64,
}

impl<C: Core> Streaming<C> {
    fn new(core: C, ch: usize, ratio: f64) -> Self {
        let to_skip = core.delay();
        Self {
            core,
            ch,
            ratio,
            inbuf: Vec::new(),
            tmp: Vec::new(),
            scratch: Vec::new(),
            to_skip,
            in_total: 0,
            out_total: 0,
        }
    }

    fn reset(&mut self) {
        self.core.reset();
        self.inbuf.clear();
        self.to_skip = self.core.delay();
        self.in_total = 0;
        self.out_total = 0;
    }

    /// Moves `tmp` to `out`, dropping the filter delay; with `cap` never exceeds that many output frames in total.
    fn emit(&mut self, out: &mut Vec<f32>, cap: Option<u64>) {
        let ch = self.ch;
        let frames = self.tmp.len() / ch;
        let skip = self.to_skip.min(frames);
        self.to_skip -= skip;
        let mut take = frames - skip;
        if let Some(cap) = cap {
            take = take.min(cap.saturating_sub(self.out_total) as usize);
        }
        out.extend_from_slice(&self.tmp[skip * ch..(skip + take) * ch]);
        self.out_total += take as u64;
    }

    /// Adds input (interleaved, already in the device channel layout) and appends all output that is ready.
    fn push(&mut self, input: &[f32], out: &mut Vec<f32>) -> Result<(), String> {
        let ch = self.ch;
        self.inbuf.extend_from_slice(input);
        self.in_total += (input.len() / ch) as u64;
        loop {
            let need = self.core.input_frames_next();
            if self.inbuf.len() < need * ch {
                return Ok(());
            }
            self.tmp.clear();
            let used = self.core.process(&self.inbuf[..need * ch], None, &mut self.tmp)?;
            if used == 0 {
                return Err("resampler consumed nothing".into());
            }
            self.inbuf.drain(..used * ch);
            self.emit(out, None);
        }
    }

    /// End of the stream: feeds silence until the delayed tail has come out, then cuts to the exact length.
    fn finish(&mut self, out: &mut Vec<f32>) -> Result<(), String> {
        let ch = self.ch;
        let expected = (self.in_total as f64 * self.ratio).round() as u64;
        for _ in 0..16 {
            if self.out_total >= expected {
                break;
            }
            let need = self.core.input_frames_next();
            let avail = (self.inbuf.len() / ch).min(need);
            self.scratch.clear();
            self.scratch.extend_from_slice(&self.inbuf[..avail * ch]);
            self.scratch.resize(need * ch, 0.0);
            self.tmp.clear();
            self.core.process(&self.scratch, Some(avail), &mut self.tmp)?;
            self.inbuf.drain(..avail * ch);
            self.emit(out, Some(expected));
        }
        self.inbuf.clear();
        Ok(())
    }
}

/// Simple linear interpolation. Only a fallback if rubato can not be created.
struct Linear {
    step: f64,
    ch: usize,
    /// Position of the next output frame, in input frames, relative to `prev`.
    pos: f64,
    /// Last frame of the previous chunk (`ch` samples); empty before the first chunk.
    prev: Vec<f32>,
}

impl Linear {
    fn new(src_rate: u32, dst_rate: u32, ch: usize) -> Self {
        Self { step: src_rate as f64 / dst_rate as f64, ch, pos: 0.0, prev: Vec::new() }
    }

    fn reset(&mut self) {
        self.pos = 0.0;
        self.prev.clear();
    }

    fn process(&mut self, frames: &[f32], out: &mut Vec<f32>) {
        let ch = self.ch;
        let n = frames.len() / ch;
        if n == 0 {
            return;
        }
        if self.prev.is_empty() {
            self.prev = frames[..ch].to_vec();
        }
        // Virtual array: index 0 = prev, index k >= 1 = frame k-1 of `frames`.
        let prev = std::mem::take(&mut self.prev);
        let get = |k: usize, c: usize| if k == 0 { prev[c] } else { frames[(k - 1) * ch + c] };
        while (self.pos as usize) < n {
            let i = self.pos as usize;
            let t = (self.pos - i as f64) as f32;
            for c in 0..ch {
                let a = get(i, c);
                let b = get(i + 1, c);
                out.push(a + (b - a) * t);
            }
            self.pos += self.step;
        }
        self.pos -= n as f64;
        self.prev = frames[(n - 1) * ch..n * ch].to_vec();
    }
}

enum Mode {
    /// Same rate: only the channel remap.
    Pass,
    Sinc(Streaming<SincCore>),
    Linear(Linear),
}

pub struct Converter {
    src_ch: usize,
    dst_ch: usize,
    src_rate: u32,
    dst_rate: u32,
    mapped: Vec<f32>,
    mode: Mode,
}

fn remap_frame(f: &[f32], dst: usize, out: &mut Vec<f32>) {
    let src = f.len();
    if src == dst {
        out.extend_from_slice(f);
    } else if src == 1 {
        out.extend(std::iter::repeat(f[0]).take(dst));
    } else if dst == 1 {
        out.push(f.iter().sum::<f32>() / src as f32);
    } else {
        // Keep the first channels, silence for missing ones (2 -> 6: front L/R only).
        for c in 0..dst {
            out.push(if c < src { f[c] } else { 0.0 });
        }
    }
}

impl Converter {
    pub fn new(src_rate: u32, src_ch: usize, dst_rate: u32, dst_ch: usize) -> Self {
        let (src_ch, dst_ch) = (src_ch.max(1), dst_ch.max(1));
        let mode = if src_rate == dst_rate || src_rate == 0 || dst_rate == 0 {
            Mode::Pass
        } else {
            match SincCore::new(src_rate, dst_rate, dst_ch, CHUNK_FRAMES) {
                Ok(core) => Mode::Sinc(Streaming::new(core, dst_ch, dst_rate as f64 / src_rate as f64)),
                Err(_) => Mode::Linear(Linear::new(src_rate, dst_rate, dst_ch)),
            }
        };
        Self { src_ch, dst_ch, src_rate, dst_rate, mapped: Vec::new(), mode }
    }

    /// Appends converted interleaved samples (device format) to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let mut mapped = std::mem::take(&mut self.mapped);
        mapped.clear();
        for f in input.chunks_exact(self.src_ch) {
            remap_frame(f, self.dst_ch, &mut mapped);
        }
        match &mut self.mode {
            Mode::Pass => out.extend_from_slice(&mapped),
            Mode::Linear(l) => l.process(&mapped, out),
            Mode::Sinc(s) => {
                if s.push(&mapped, out).is_err() {
                    // Should never happen. Hand the not yet consumed audio to the linear fallback and go on.
                    let rest = std::mem::take(&mut s.inbuf);
                    let mut lin = Linear::new(self.src_rate, self.dst_rate, self.dst_ch);
                    lin.process(&rest, out);
                    self.mode = Mode::Linear(lin);
                }
            }
        }
        self.mapped = mapped;
    }

    /// End of the track: appends the audio still inside the filter. Call once, then drop or `reset` the converter.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        if let Mode::Sinc(s) = &mut self.mode {
            let _ = s.finish(out);
        }
    }

    /// Back to the start state (after a seek): no old audio is mixed into the new position.
    pub fn reset(&mut self) {
        match &mut self.mode {
            Mode::Pass => {}
            Mode::Sinc(s) => s.reset(),
            Mode::Linear(l) => l.reset(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    // ---- Converter with the real rubato core (runs in CI) ----

    #[test]
    fn same_format_is_passthrough() {
        let mut c = Converter::new(44_100, 2, 44_100, 2);
        let mut out = Vec::new();
        c.process(&[0.1, 0.2, 0.3, 0.4], &mut out);
        c.finish(&mut out);
        assert_eq!(out, [0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn mono_is_duplicated_to_stereo() {
        let mut c = Converter::new(44_100, 1, 44_100, 2);
        let mut out = Vec::new();
        c.process(&[0.5, -0.5], &mut out);
        assert_eq!(out, [0.5, 0.5, -0.5, -0.5]);
    }

    #[test]
    fn downsampling_gives_exact_length() {
        let mut c = Converter::new(44_100, 1, 22_050, 1);
        let input: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        let mut out = Vec::new();
        c.process(&input, &mut out);
        c.finish(&mut out);
        assert_eq!(out.len(), 500);
    }

    fn sine_stereo(freq: f64, amp: f64, rate: u32, frames: usize) -> Vec<f32> {
        let mut v = Vec::with_capacity(frames * 2);
        for n in 0..frames {
            let s = (amp * (2.0 * PI * freq * n as f64 / rate as f64).sin()) as f32;
            v.push(s);
            v.push(s);
        }
        v
    }

    fn convert_in_pieces(c: &mut Converter, input: &[f32], piece_frames: usize) -> Vec<f32> {
        let mut out = Vec::new();
        for p in input.chunks(piece_frames * 2) {
            c.process(p, &mut out);
        }
        c.finish(&mut out);
        out
    }

    /// 1 kHz sine 44.1 -> 48 kHz: exact length, aligned in time (delay trimmed), same shape as an ideal 48 kHz sine.
    #[test]
    fn sine_is_resampled_accurately_and_aligned() {
        let input = sine_stereo(1000.0, 0.5, 44_100, 88_200);
        let mut c = Converter::new(44_100, 2, 48_000, 2);
        let out = convert_in_pieces(&mut c, &input, 1000);
        assert_eq!(out.len() / 2, 96_000);
        let mut worst = 0.0_f64;
        for n in 2000..(96_000 - 2000) {
            let ideal = 0.5 * (2.0 * PI * 1000.0 * n as f64 / 48_000.0).sin();
            worst = worst.max((out[n * 2] as f64 - ideal).abs());
            assert_eq!(out[n * 2], out[n * 2 + 1], "channels must stay identical");
        }
        assert!(worst < 0.01, "max error {worst}");
    }

    #[test]
    fn upsampling_to_96k_keeps_tone() {
        let input = sine_stereo(440.0, 0.4, 44_100, 44_100);
        let mut c = Converter::new(44_100, 2, 96_000, 2);
        let out = convert_in_pieces(&mut c, &input, 4096);
        let frames = out.len() / 2;
        assert_eq!(frames, (44_100.0_f64 * 96_000.0 / 44_100.0).round() as usize);
        let mut worst = 0.0_f64;
        for n in 3000..(frames - 3000) {
            let ideal = 0.4 * (2.0 * PI * 440.0 * n as f64 / 96_000.0).sin();
            worst = worst.max((out[n * 2] as f64 - ideal).abs());
        }
        assert!(worst < 0.01, "max error {worst}");
    }

    #[test]
    fn piece_size_does_not_change_the_result() {
        let input = sine_stereo(700.0, 0.3, 48_000, 30_000);
        let a = convert_in_pieces(&mut Converter::new(48_000, 2, 44_100, 2), &input, 7);
        let b = convert_in_pieces(&mut Converter::new(48_000, 2, 44_100, 2), &input, 30_000);
        assert_eq!(a, b);
    }

    #[test]
    fn reset_restarts_cleanly() {
        let input = sine_stereo(1000.0, 0.5, 44_100, 20_000);
        let mut c = Converter::new(44_100, 2, 48_000, 2);
        let first = convert_in_pieces(&mut c, &input, 1000);
        c.reset();
        let second = convert_in_pieces(&mut c, &input, 1000);
        assert_eq!(first, second);
    }

    #[test]
    fn linear_fallback_halves_length() {
        let mut l = Linear::new(44_100, 22_050, 1);
        let input: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let mut out = Vec::new();
        l.process(&input, &mut out);
        assert!((out.len() as i64 - 500).abs() <= 2);
    }

    // ---- Streaming wrapper with a fake core (checks the bookkeeping only) ----

    /// Nearest-neighbour "resampler" with a configurable delay and fixed-size input chunks.
    struct FakeCore {
        ratio: f64,
        chunk: usize,
        delay: usize,
        ch: usize,
        all: Vec<f32>,
        consumed: u64,
        emitted: u64,
    }

    impl FakeCore {
        fn new(ratio: f64, chunk: usize, delay: usize, ch: usize) -> Self {
            Self { ratio, chunk, delay, ch, all: Vec::new(), consumed: 0, emitted: 0 }
        }
    }

    impl Core for FakeCore {
        fn input_frames_next(&self) -> usize {
            self.chunk
        }
        fn delay(&self) -> usize {
            self.delay
        }
        fn reset(&mut self) {
            self.all.clear();
            self.consumed = 0;
            self.emitted = 0;
        }
        fn process(&mut self, input: &[f32], partial: Option<usize>, out: &mut Vec<f32>) -> Result<usize, String> {
            let n = partial.unwrap_or(self.chunk);
            self.all.extend_from_slice(&input[..self.chunk * self.ch]);
            self.consumed += self.chunk as u64;
            let target = (self.delay as f64 + self.consumed as f64 * self.ratio).floor() as u64;
            while self.emitted < target {
                let k = self.emitted as usize;
                for c in 0..self.ch {
                    out.push(if k < self.delay {
                        0.0
                    } else {
                        let idx = ((k - self.delay) as f64 / self.ratio).floor() as usize;
                        self.all[idx * self.ch + c]
                    });
                }
                self.emitted += 1;
            }
            Ok(n)
        }
    }

    fn run_fake(ratio: f64, delay: usize, input: &[f32], piece_frames: usize) -> Vec<f32> {
        let mut s = Streaming::new(FakeCore::new(ratio, 64, delay, 1), 1, ratio);
        let mut out = Vec::new();
        for p in input.chunks(piece_frames) {
            s.push(p, &mut out).unwrap();
        }
        s.finish(&mut out).unwrap();
        out
    }

    #[test]
    fn fake_length_is_exact_for_several_ratios() {
        let input: Vec<f32> = (0..10_000).map(|i| i as f32).collect();
        for &(ratio, delay) in &[(0.5, 17), (2.0, 40), (48_000.0 / 44_100.0, 63), (1.0 / 3.0, 5)] {
            let out = run_fake(ratio, delay, &input, 1000);
            assert_eq!(out.len(), (10_000.0 * ratio).round() as usize, "ratio {ratio}");
        }
    }

    #[test]
    fn fake_delay_is_trimmed_from_the_start() {
        let input: Vec<f32> = (1..=5000).map(|i| i as f32).collect();
        let out = run_fake(2.0, 33, &input, 500);
        assert_eq!(out[0], 1.0); // first output = first input, not silence
        assert_eq!(out[1], 1.0);
        assert_eq!(out[2], 2.0);
    }

    #[test]
    fn fake_piece_size_is_irrelevant() {
        let input: Vec<f32> = (0..7_777).map(|i| (i % 91) as f32).collect();
        assert_eq!(run_fake(0.75, 20, &input, 1), run_fake(0.75, 20, &input, 7_777));
    }

    #[test]
    fn fake_short_input_still_comes_out() {
        // Less than one chunk: nothing during push, everything at finish.
        let input: Vec<f32> = (0..10).map(|i| i as f32).collect();
        let mut s = Streaming::new(FakeCore::new(2.0, 64, 9, 1), 1, 2.0);
        let mut out = Vec::new();
        s.push(&input, &mut out).unwrap();
        assert!(out.is_empty());
        s.finish(&mut out).unwrap();
        assert_eq!(out.len(), 20);
    }

    #[test]
    fn fake_reset_gives_the_same_output_again() {
        let input: Vec<f32> = (0..3000).map(|i| (i % 50) as f32).collect();
        let mut s = Streaming::new(FakeCore::new(1.5, 64, 12, 1), 1, 1.5);
        let mut a = Vec::new();
        s.push(&input, &mut a).unwrap();
        s.finish(&mut a).unwrap();
        s.reset();
        let mut b = Vec::new();
        s.push(&input, &mut b).unwrap();
        s.finish(&mut b).unwrap();
        assert_eq!(a, b);
    }
}
