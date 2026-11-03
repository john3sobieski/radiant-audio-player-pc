//! Pitch-preserving speed change: a streaming phase vocoder with identity phase locking.
//! Port of Android `TimeStretch.kt` (`TimeStretchProcessor` + `PhaseVocoder`), same numbers and same structure.
//!
//! Why not a classic WSOLA/Sonic: on music they "chop" the sound when slowing down. Here every spectrum frame
//! (Hann window, ~90 ms) is played longer and phases of all bins of one spectral peak move together, so there are
//! no clicks and no metallic tone. Stereo pairs use one phase shift (computed from L+R) for both channels.
//!
//! The FFT is the hand-written radix-2 from the Kotlin code (kept so the port is identical and testable
//! without extra crates). It can be swapped for `realfft` later if measurements show a need.
//!
//! API: interleaved `f32` in, interleaved `f32` out, no audio device. Speed 1.0 before the first change is a
//! zero-cost pass-through; the engine starts on the fly from the current position (no gap).

use std::f64::consts::PI;

/// Allowed speed range (same clamp as the Android code).
pub const MIN_SPEED: f32 = 0.25;
pub const MAX_SPEED: f32 = 4.0;

/// Maps the UI slider position `t` (-2 ..= +2, step 0.1) to a speed: `1 + t` for t >= 0, `1 / (1 - t)` for t < 0.
pub fn slider_to_speed(t: f32) -> f32 {
    if t >= 0.0 {
        1.0 + t
    } else {
        1.0 / (1.0 - t)
    }
}

/// Speed processor for interleaved audio. One instance per stream format.
pub struct TimeStretch {
    channels: usize,
    sample_rate: u32,
    speed: f32,
    engine: Option<PhaseVocoder>,
}

impl TimeStretch {
    pub fn new(channels: usize, sample_rate: u32) -> Self {
        Self { channels: channels.max(1), sample_rate: sample_rate.max(8000), speed: 1.0, engine: None }
    }

    pub fn speed(&self) -> f32 {
        self.speed
    }

    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(MIN_SPEED, MAX_SPEED);
    }

    /// Drops internal state (seek, new track). Next input starts fresh.
    pub fn reset(&mut self) {
        self.engine = None;
    }

    /// Changes the stream format; drops internal state.
    pub fn configure(&mut self, channels: usize, sample_rate: u32) {
        self.channels = channels.max(1);
        self.sample_rate = sample_rate.max(8000);
        self.engine = None;
    }

    /// True when `process` would copy audio through without any work.
    pub fn is_passthrough(&self) -> bool {
        self.engine.is_none() && (self.speed - 1.0).abs() < 0.001
    }

    /// Feeds interleaved samples; appends the ready output (interleaved) to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let frames = input.len() / self.channels;
        if frames == 0 {
            return;
        }
        if self.engine.is_none() {
            if (self.speed - 1.0).abs() < 0.001 {
                out.extend_from_slice(input);
                return;
            }
            self.engine = Some(PhaseVocoder::new(self.channels, self.sample_rate));
        }
        let speed = self.speed;
        let eng = self.engine.as_mut().unwrap();
        eng.push_input(&input[..frames * self.channels], frames);
        eng.process(speed, false);
        eng.drain_to(out);
    }

    /// End of stream: appends silence, plays out the tail and appends everything left to `out`.
    pub fn finish(&mut self, out: &mut Vec<f32>) {
        let speed = self.speed;
        if let Some(eng) = self.engine.as_mut() {
            eng.process(speed, true);
            eng.drain_to(out);
        }
    }
}

/// Immutable tables and sizes shared by all channel groups.
struct Shared {
    n: usize,
    half: usize,
    hs: usize,
    window: Vec<f64>,
    window2: Vec<f64>,
    cos_t: Vec<f64>,
    sin_t: Vec<f64>,
    rev: Vec<usize>,
}

impl Shared {
    fn new(sample_rate: u32) -> Self {
        let log2 = (sample_rate as f64 * 0.09).ln() / 2f64.ln();
        let n = (1usize << (log2.round() as i32).clamp(0, 30) as u32).clamp(2048, 16384);
        let half = n / 2;
        let window: Vec<f64> = (0..n).map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / n as f64).cos()).collect();
        let window2 = window.iter().map(|w| w * w).collect();
        let cos_t = (0..half).map(|i| (2.0 * PI * i as f64 / n as f64).cos()).collect();
        let sin_t = (0..half).map(|i| (2.0 * PI * i as f64 / n as f64).sin()).collect();
        let bits = n.trailing_zeros();
        let rev = (0..n).map(|i| ((i as u32).reverse_bits() >> (32 - bits)) as usize).collect();
        Self { n, half, hs: n / 4, window, window2, cos_t, sin_t, rev }
    }

    /// Forward FFT (radix-2, in place) of length n.
    fn fft(&self, xr: &mut [f64], xi: &mut [f64]) {
        let n = self.n;
        for i in 0..n {
            let j = self.rev[i];
            if j > i {
                xr.swap(i, j);
                xi.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half_len = len >> 1;
            let step = n / len;
            let mut i = 0;
            while i < n {
                let mut t = 0;
                for k in 0..half_len {
                    let wr = self.cos_t[t];
                    let wi = -self.sin_t[t];
                    let p = i + k;
                    let q = p + half_len;
                    let tr = xr[q] * wr - xi[q] * wi;
                    let ti = xr[q] * wi + xi[q] * wr;
                    xr[q] = xr[p] - tr;
                    xi[q] = xi[p] - ti;
                    xr[p] += tr;
                    xi[p] += ti;
                    t += step;
                }
                i += len;
            }
            len <<= 1;
        }
    }
}

/// Streaming phase vocoder. Hann window ~90 ms, synthesis hop N/4, analysis hop (N/4) * speed.
struct PhaseVocoder {
    channels: usize,
    sh: Shared,
    // Input queue per channel; `in_base` = absolute index of the sample at in_buf[..][0].
    in_buf: Vec<Vec<f32>>,
    in_base: i64,
    in_len: usize,
    total_in: i64,
    ana_pos: f64,
    prev_start: i64,
    finished: bool,
    // Overlap-add accumulator and the running sum of squared windows.
    acc: Vec<Vec<f64>>,
    nrm: Vec<f64>,
    // Ready output per channel.
    out: Vec<Vec<f32>>,
    out_frames: usize,
    groups: Vec<Group>,
}

impl PhaseVocoder {
    fn new(channels: usize, sample_rate: u32) -> Self {
        let sh = Shared::new(sample_rate);
        let n = sh.n;
        let groups = (0..(channels + 1) / 2)
            .map(|g| {
                let a = g * 2;
                Group::new(a, if a + 1 < channels { Some(a + 1) } else { None }, n)
            })
            .collect();
        Self {
            channels,
            in_buf: vec![vec![0.0; n * 4]; channels],
            in_base: 0,
            in_len: 0,
            total_in: 0,
            ana_pos: 0.0,
            prev_start: -1,
            finished: false,
            acc: vec![vec![0.0; n]; channels],
            nrm: vec![0.0; n],
            out: vec![vec![0.0; n * 4]; channels],
            out_frames: 0,
            groups,
            sh,
        }
    }

    fn ensure_in(&mut self, need: usize) {
        if self.in_buf[0].len() >= need {
            return;
        }
        let size = need.max(self.in_buf[0].len() * 2);
        for c in 0..self.channels {
            self.in_buf[c].resize(size, 0.0);
        }
    }

    fn ensure_out(&mut self, need: usize) {
        if self.out[0].len() >= need {
            return;
        }
        let size = need.max(self.out[0].len() * 2);
        for c in 0..self.channels {
            self.out[c].resize(size, 0.0);
        }
    }

    fn push_input(&mut self, input: &[f32], frames: usize) {
        self.ensure_in(self.in_len + frames);
        for f in 0..frames {
            for c in 0..self.channels {
                self.in_buf[c][self.in_len + f] = input[f * self.channels + c];
            }
        }
        self.in_len += frames;
        self.total_in += frames as i64;
    }

    /// Moves ready audio (interleaved) to `out` and empties the internal output queue.
    fn drain_to(&mut self, out: &mut Vec<f32>) {
        out.reserve(self.out_frames * self.channels);
        for f in 0..self.out_frames {
            for c in 0..self.channels {
                out.push(self.out[c][f]);
            }
        }
        self.out_frames = 0;
    }

    /// Processes every frame for which enough input is available. With `finish` it appends silence,
    /// plays out the tail and emits everything that is left.
    fn process(&mut self, speed: f32, finish: bool) {
        let n = self.sh.n;
        if finish {
            if self.finished {
                return;
            }
            self.finished = true;
            self.ensure_in(self.in_len + n);
            for c in 0..self.channels {
                let len = self.in_len;
                self.in_buf[c][len..len + n].fill(0.0);
            }
            self.in_len += n;
        }

        let step = self.sh.hs as f64 * speed as f64;
        loop {
            let s = (self.ana_pos + 0.5).floor() as i64;
            if finish {
                if s >= self.total_in {
                    break;
                }
            } else if s + n as i64 > self.in_base + self.in_len as i64 {
                break;
            }
            self.process_frame((s - self.in_base) as usize, s);
            self.ana_pos += step;
        }

        if finish {
            self.emit_tail();
        } else {
            // Drop what the next frame no longer touches.
            let keep = (((self.ana_pos + 0.5).floor() as i64) - self.in_base).clamp(0, self.in_len as i64) as usize;
            if keep > 0 {
                for c in 0..self.channels {
                    self.in_buf[c].copy_within(keep..self.in_len, 0);
                }
                self.in_base += keep as i64;
                self.in_len -= keep;
            }
        }
    }

    fn process_frame(&mut self, offset: usize, start: i64) {
        let (n, hs) = (self.sh.n, self.sh.hs);
        let ha = if self.prev_start < 0 { hs } else { ((start - self.prev_start) as usize).max(1) };
        for g in 0..self.groups.len() {
            let (a, b) = (self.groups[g].a, self.groups[g].b);
            let la = &self.in_buf[a][offset..offset + n];
            let lb = b.map(|b| &self.in_buf[b][offset..offset + n]);
            // Accumulators of one group: a < b, so split to borrow both mutably.
            let (acc_a, acc_b) = match b {
                Some(b) => {
                    let (left, right) = self.acc.split_at_mut(b);
                    (&mut left[a], Some(&mut right[0]))
                }
                None => (&mut self.acc[a], None),
            };
            self.groups[g].process(&self.sh, la, lb, acc_a, acc_b, ha);
        }
        for i in 0..n {
            self.nrm[i] += self.sh.window2[i];
        }
        self.prev_start = start;

        // The first hs samples of the accumulator are final: emit them and shift.
        self.ensure_out(self.out_frames + hs);
        let of = self.out_frames;
        for i in 0..hs {
            let g = if self.nrm[i] > 1e-9 { 1.0 / self.nrm[i] } else { 0.0 };
            for c in 0..self.channels {
                self.out[c][of + i] = (self.acc[c][i] * g) as f32;
            }
        }
        self.out_frames += hs;
        for c in 0..self.channels {
            let a = &mut self.acc[c];
            a.copy_within(hs..n, 0);
            a[n - hs..].fill(0.0);
        }
        self.nrm.copy_within(hs..n, 0);
        self.nrm[n - hs..].fill(0.0);
    }

    /// End of stream: emit the tail while at least part of a frame still overlaps it.
    fn emit_tail(&mut self) {
        let n = self.sh.n;
        let mut cnt = 0;
        while cnt < n && self.nrm[cnt] > 1.0 {
            cnt += 1;
        }
        self.ensure_out(self.out_frames + cnt);
        let of = self.out_frames;
        for c in 0..self.channels {
            for i in 0..cnt {
                self.out[c][of + i] = (self.acc[c][i] / self.nrm[i]) as f32;
            }
            self.acc[c].fill(0.0);
        }
        self.nrm.fill(0.0);
        self.out_frames += cnt;
    }
}

/// One stereo pair (or a single channel): one shared phase shift for both channels.
struct Group {
    a: usize,
    b: Option<usize>,
    re: Vec<f64>,
    im: Vec<f64>,
    // Spectra of the channels (half spectrum 0..=half), the sum used for phases, and its previous value.
    xlr: Vec<f64>,
    xli: Vec<f64>,
    xrr: Vec<f64>,
    xri: Vec<f64>,
    mr: Vec<f64>,
    mi: Vec<f64>,
    mr_prev: Vec<f64>,
    mi_prev: Vec<f64>,
    pw: Vec<f64>,
    // Phase shift applied on the previous frame and on this one (per bin).
    th_prev: Vec<f64>,
    th: Vec<f64>,
    rc: Vec<f64>,
    rs: Vec<f64>,
    peaks: Vec<usize>,
    has_prev: bool,
}

/// Wraps an angle to [-pi, pi].
fn wrap(x: f64) -> f64 {
    let mut v = x % (2.0 * PI);
    if v > PI {
        v -= 2.0 * PI;
    } else if v < -PI {
        v += 2.0 * PI;
    }
    v
}

impl Group {
    fn new(a: usize, b: Option<usize>, n: usize) -> Self {
        let h = n / 2 + 1;
        Self {
            a,
            b,
            re: vec![0.0; n],
            im: vec![0.0; n],
            xlr: vec![0.0; h],
            xli: vec![0.0; h],
            xrr: vec![0.0; h],
            xri: vec![0.0; h],
            mr: vec![0.0; h],
            mi: vec![0.0; h],
            mr_prev: vec![0.0; h],
            mi_prev: vec![0.0; h],
            pw: vec![0.0; h],
            th_prev: vec![0.0; h],
            th: vec![0.0; h],
            rc: vec![0.0; h],
            rs: vec![0.0; h],
            peaks: vec![0; h],
            has_prev: false,
        }
    }

    fn process(
        &mut self,
        sh: &Shared,
        la: &[f32],
        lb: Option<&[f32]>,
        acc_a: &mut [f64],
        acc_b: Option<&mut Vec<f64>>,
        ha: usize,
    ) {
        let (n, half, hs) = (sh.n, sh.half, sh.hs);
        let stereo = lb.is_some();
        for i in 0..n {
            let w = sh.window[i];
            self.re[i] = la[i] as f64 * w;
            self.im[i] = lb.map_or(0.0, |l| l[i] as f64 * w);
        }
        sh.fft(&mut self.re, &mut self.im);

        // Split the spectra of the pair packed into one complex signal L + jR.
        for k in 0..=half {
            let nk = (n - k) & (n - 1);
            let zr = self.re[k];
            let zi = self.im[k];
            if stereo {
                let yr = self.re[nk];
                let yi = self.im[nk];
                self.xlr[k] = (zr + yr) * 0.5;
                self.xli[k] = (zi - yi) * 0.5;
                self.xrr[k] = (zi + yi) * 0.5;
                self.xri[k] = -(zr - yr) * 0.5;
                self.mr[k] = self.xlr[k] + self.xrr[k];
                self.mi[k] = self.xli[k] + self.xri[k];
            } else {
                self.xlr[k] = zr;
                self.xli[k] = zi;
                self.mr[k] = zr;
                self.mi[k] = zi;
            }
            self.pw[k] = self.mr[k] * self.mr[k] + self.mi[k] * self.mi[k];
        }

        // Spectral peaks of the channel sum.
        let mut np = 0;
        for k in 0..=half {
            let p = self.pw[k];
            if p <= 1e-18 {
                continue;
            }
            let mut ok = true;
            for d in 1..=2 {
                if k >= d && self.pw[k - d] >= p {
                    ok = false;
                }
                if k + d <= half && self.pw[k + d] > p {
                    ok = false;
                }
            }
            if ok {
                self.peaks[np] = k;
                np += 1;
            }
        }

        // Phase shift per peak; other bins take the shift of the nearest peak.
        self.rc.fill(1.0);
        self.rs.fill(0.0);
        self.th.fill(0.0);
        if self.has_prev && np > 0 {
            let ratio = hs as f64 / ha as f64;
            for i in 0..np {
                let k = self.peaks[i];
                let pr = self.mr[k] * self.mr_prev[k] + self.mi[k] * self.mi_prev[k];
                let pi = self.mi[k] * self.mr_prev[k] - self.mr[k] * self.mi_prev[k];
                let d0 = pi.atan2(pr);
                let omega = 2.0 * PI * k as f64 / n as f64;
                let d = wrap(d0 - omega * ha as f64);
                let t = wrap(self.th_prev[k] + ratio * d + omega * hs as f64 - d0);
                let (s, c) = t.sin_cos();
                let from = if i == 0 { 0 } else { (self.peaks[i - 1] + k + 1) / 2 };
                let to = if i == np - 1 { half + 1 } else { (k + self.peaks[i + 1] + 1) / 2 };
                for j in from..to {
                    self.th[j] = t;
                    self.rc[j] = c;
                    self.rs[j] = s;
                }
            }
        }
        // Remember the state for the next frame.
        std::mem::swap(&mut self.th_prev, &mut self.th);
        std::mem::swap(&mut self.mr_prev, &mut self.mr);
        std::mem::swap(&mut self.mi_prev, &mut self.mi);
        self.has_prev = true;

        // Rotate the spectra and pack L + jR for the inverse FFT.
        for k in 0..=half {
            let (mut c, mut s) = (self.rc[k], self.rs[k]);
            if k == 0 || k == half {
                // DC and Nyquist must stay real.
                c = 1.0;
                s = 0.0;
            }
            let ylr = self.xlr[k] * c - self.xli[k] * s;
            let mut yli = self.xlr[k] * s + self.xli[k] * c;
            let (mut yrr, mut yri) = (0.0, 0.0);
            if stereo {
                yrr = self.xrr[k] * c - self.xri[k] * s;
                yri = self.xrr[k] * s + self.xri[k] * c;
            }
            if k == 0 || k == half {
                yli = 0.0;
                yri = 0.0;
            }
            self.re[k] = ylr - yri;
            self.im[k] = yli + yrr;
            if k >= 1 && k < half {
                let j = n - k;
                self.re[j] = ylr + yri;
                self.im[j] = -yli + yrr;
            }
        }

        // Inverse FFT through the forward one with re/im swapped: re = left, im = right.
        sh.fft(&mut self.im, &mut self.re);
        let inv = 1.0 / n as f64;
        for i in 0..n {
            acc_a[i] += self.re[i] * sh.window[i] * inv;
        }
        if let Some(acc_b) = acc_b {
            for i in 0..n {
                acc_b[i] += self.im[i] * sh.window[i] * inv;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: u32 = 44_100;

    fn sine(freq: f64, amp: f64, frames: usize, stereo: bool) -> Vec<f32> {
        let mut v = Vec::new();
        for n in 0..frames {
            let s = (amp * (2.0 * PI * freq * n as f64 / FS as f64).sin()) as f32;
            v.push(s);
            if stereo {
                v.push(s);
            }
        }
        v
    }

    fn run(speed: f32, input: &[f32], channels: usize, chunk_frames: usize) -> Vec<f32> {
        let mut st = TimeStretch::new(channels, FS);
        st.set_speed(speed);
        let mut out = Vec::new();
        for c in input.chunks(chunk_frames * channels) {
            st.process(c, &mut out);
        }
        st.finish(&mut out);
        out
    }

    /// Dominant frequency by scanning Goertzel bins in 1 Hz steps (on the steady middle part).
    fn dominant_hz(mono: &[f32], lo: u32, hi: u32) -> u32 {
        let mut best = (0.0, lo);
        for f in lo..=hi {
            let w = 2.0 * PI * f as f64 / FS as f64;
            let coeff = 2.0 * w.cos();
            let (mut s1, mut s2) = (0.0, 0.0);
            for &x in mono {
                let s = x as f64 + coeff * s1 - s2;
                s2 = s1;
                s1 = s;
            }
            let p = s1 * s1 + s2 * s2 - coeff * s1 * s2;
            if p > best.0 {
                best = (p, f);
            }
        }
        best.1
    }

    fn rms(x: &[f32]) -> f64 {
        (x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt()
    }

    #[test]
    fn slider_mapping_matches_readme() {
        assert_eq!(slider_to_speed(0.0), 1.0);
        assert_eq!(slider_to_speed(2.0), 3.0);
        assert!((slider_to_speed(-2.0) - 1.0 / 3.0).abs() < 1e-6);
        assert!((slider_to_speed(-1.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn speed_one_is_untouched_passthrough() {
        let input = sine(440.0, 0.5, 5000, true);
        let mut st = TimeStretch::new(2, FS);
        assert!(st.is_passthrough());
        let mut out = Vec::new();
        st.process(&input, &mut out);
        assert_eq!(out, input);
    }

    #[test]
    fn pitch_is_kept_and_length_scales() {
        let frames = FS as usize * 3;
        for &speed in &[0.5_f32, 0.75, 1.5, 2.0, 3.0] {
            let input = sine(440.0, 0.5, frames, true);
            let out = run(speed, &input, 2, 1000);
            let out_frames = out.len() / 2;
            let expect = frames as f64 / speed as f64;
            // Within one window (4096) of the ideal length.
            assert!(
                (out_frames as f64 - expect).abs() < 4096.0 * 1.5,
                "speed {speed}: got {out_frames}, expected {expect}"
            );
            let left: Vec<f32> = out.chunks_exact(2).map(|f| f[0]).collect();
            let mid = &left[8192..left.len() - 8192];
            let f = dominant_hz(&mid[..mid.len().min(30_000)], 420, 460);
            assert_eq!(f, 440, "speed {speed}: pitch moved to {f} Hz");
            let r = rms(mid);
            let want = 0.5 / 2f64.sqrt();
            assert!((r - want).abs() / want < 0.1, "speed {speed}: rms {r} vs {want}");
        }
    }

    #[test]
    fn output_has_no_clicks() {
        let input = sine(440.0, 0.5, FS as usize * 2, true);
        for &speed in &[0.5_f32, 1.7] {
            let out = run(speed, &input, 2, 777);
            let left: Vec<f32> = out.chunks_exact(2).map(|f| f[0]).collect();
            // A clean sine can not change faster than 2*pi*f/fs*amp per sample.
            let max_step = (2.0 * PI * 440.0 / FS as f64 * 0.5 * 1.3) as f32;
            let worst = left[4096..left.len() - 4096]
                .windows(2)
                .map(|w| (w[1] - w[0]).abs())
                .fold(0.0_f32, f32::max);
            assert!(worst < max_step, "speed {speed}: jump {worst} > {max_step}");
        }
    }

    #[test]
    fn chunk_size_does_not_change_the_result() {
        let input = sine(330.0, 0.4, FS as usize, true);
        let a = run(0.8, &input, 2, 64);
        let b = run(0.8, &input, 2, 100_000);
        assert_eq!(a, b);
    }

    #[test]
    fn stereo_image_is_kept() {
        // R = half of L: the ratio must survive the stretch (same phase shift for both channels).
        let frames = FS as usize * 2;
        let mut input = Vec::new();
        for n in 0..frames {
            let s = (0.6 * (2.0 * PI * 523.0 * n as f64 / FS as f64).sin()) as f32;
            input.push(s);
            input.push(s * 0.5);
        }
        let out = run(0.6, &input, 2, 1024);
        let l: Vec<f32> = out.chunks_exact(2).map(|f| f[0]).collect();
        let r: Vec<f32> = out.chunks_exact(2).map(|f| f[1]).collect();
        let (l, r) = (&l[8192..l.len() - 8192], &r[8192..r.len() - 8192]);
        let ratio = rms(r) / rms(l);
        assert!((ratio - 0.5).abs() < 0.02, "channel ratio {ratio}");
        // And they stay in phase: L*R correlation is positive and close to the product of RMS values.
        let corr: f64 = l.iter().zip(r).map(|(a, b)| (*a as f64) * (*b as f64)).sum::<f64>() / l.len() as f64;
        assert!(corr / (rms(l) * rms(r)) > 0.98);
    }

    #[test]
    fn mono_and_multichannel_work() {
        let mono = sine(440.0, 0.5, FS as usize, false);
        let out = run(1.5, &mono, 1, 500);
        assert!(out.len() > FS as usize / 2);

        // 3 channels: one pair plus one single channel.
        let mut three = Vec::new();
        for &s in &sine(440.0, 0.5, FS as usize, false) {
            three.extend_from_slice(&[s, s, s]);
        }
        let out3 = run(0.5, &three, 3, 500);
        assert_eq!(out3.len() % 3, 0);
        assert!(out3.len() > FS as usize);
    }

    #[test]
    fn silence_stays_silent_and_nothing_is_nan() {
        let out = run(0.5, &vec![0.0; FS as usize * 2], 2, 1000);
        assert!(out.iter().all(|v| *v == 0.0));
        let noise: Vec<f32> = (0..FS as usize * 2).map(|i| ((i * 7919 % 2001) as f32 / 1000.0) - 1.0).collect();
        let out = run(2.5, &noise, 2, 333);
        assert!(out.iter().all(|v| v.is_finite()));
    }

    #[test]
    fn speed_can_change_on_the_fly() {
        let input = sine(440.0, 0.5, FS as usize * 2, true);
        let mut st = TimeStretch::new(2, FS);
        let mut out = Vec::new();
        let half = input.len() / 2;
        st.process(&input[..half], &mut out); // passthrough part
        let pass = out.len();
        assert_eq!(pass, half);
        st.set_speed(0.5);
        st.process(&input[half..], &mut out);
        st.finish(&mut out);
        assert!(out.len() > pass + half); // slower => the second half became longer
        assert!(out.iter().all(|v| v.is_finite()));
    }
}
