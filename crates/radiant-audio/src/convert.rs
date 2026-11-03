//! Channel remap + linear resampler (source format -> device format).
//! TODO: replace the linear interpolation with `rubato` for better quality.

pub struct Converter {
    src_rate: u32,
    dst_rate: u32,
    src_ch: usize,
    dst_ch: usize,
    /// Position of the next output frame, in input frames, relative to `prev`.
    pos: f64,
    /// Last frame of the previous chunk (dst_ch samples); empty before the first chunk.
    prev: Vec<f32>,
    mapped: Vec<f32>,
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
        Self {
            src_rate,
            dst_rate,
            src_ch: src_ch.max(1),
            dst_ch: dst_ch.max(1),
            pos: 0.0,
            prev: Vec::new(),
            mapped: Vec::new(),
        }
    }

    /// Appends converted interleaved samples (device format) to `out`.
    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let mut mapped = std::mem::take(&mut self.mapped);
        mapped.clear();
        for f in input.chunks_exact(self.src_ch) {
            remap_frame(f, self.dst_ch, &mut mapped);
        }
        if self.src_rate == self.dst_rate || self.src_rate == 0 || self.dst_rate == 0 {
            out.extend_from_slice(&mapped);
        } else {
            self.resample(&mapped, out);
        }
        self.mapped = mapped;
    }

    fn resample(&mut self, frames: &[f32], out: &mut Vec<f32>) {
        let ch = self.dst_ch;
        let n = frames.len() / ch;
        if n == 0 {
            return;
        }
        if self.prev.is_empty() {
            self.prev = frames[..ch].to_vec();
        }
        let step = self.src_rate as f64 / self.dst_rate as f64;
        // Virtual array: index 0 = prev, index k >= 1 = frame k-1 of `frames`.
        let prev = std::mem::take(&mut self.prev);
        let get = |k: usize, c: usize| if k == 0 { prev[c] } else { frames[(k - 1) * ch + c] };
        while (self.pos as usize) + 1 <= n {
            let i = self.pos as usize;
            let t = (self.pos - i as f64) as f32;
            for c in 0..ch {
                let a = get(i, c);
                let b = get(i + 1, c);
                out.push(a + (b - a) * t);
            }
            self.pos += step;
        }
        self.pos -= n as f64;
        self.prev = frames[(n - 1) * ch..n * ch].to_vec();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_format_is_passthrough() {
        let mut c = Converter::new(44_100, 2, 44_100, 2);
        let mut out = Vec::new();
        c.process(&[0.1, 0.2, 0.3, 0.4], &mut out);
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
    fn downsampling_halves_length() {
        let mut c = Converter::new(44_100, 1, 22_050, 1);
        let input: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let mut out = Vec::new();
        c.process(&input, &mut out);
        assert!((out.len() as i64 - 500).abs() <= 2);
    }
}
