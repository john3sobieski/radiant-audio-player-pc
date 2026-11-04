//! Geometry of the spectrum rays around the progress ring.
//!
//! Pure functions: spectrum bands in, SVG path strings out (one `Path` element per layer in the
//! Slint file). Coordinates are in a 1000 x 1000 box that Slint scales to the real size, so the
//! same strings work for every window size. The ring itself has radius `RING_R`.

/// Rays around the full circle (an even number: the picture is mirrored left / right).
pub const RAY_COUNT: usize = 120;
/// Ring radius in box units; the Slint file sizes the box to `ring-d * 1.4` to match.
pub const RING_R: f32 = 357.0;
/// Where the rays start (a small gap from the ring) and how long the longest one can get.
const R_INNER: f32 = RING_R + 16.0;
const R_SPAN: f32 = 118.0;
/// Idle ray length: the ring stays decorated when it is quiet.
const MIN_LEN: f32 = 5.0;
/// The outer part of each ray that is drawn again in a brighter color.
const TIP_FRACTION: f32 = 0.34;

#[derive(Default, Clone, PartialEq)]
pub struct RayPaths {
    /// Wide, faint copy of the rays (soft glow).
    pub halo: String,
    /// The rays themselves.
    pub body: String,
    /// Bright outer end of every ray.
    pub tips: String,
    /// Small falling markers above the rays.
    pub peaks: String,
}

/// Level of ray `j`: lows sit at the bottom of the circle, highs at the top, mirrored left / right.
/// The value is interpolated between neighbouring bands so the outline is round, not stepped.
fn level_at(values: &[f32], j: usize) -> f32 {
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    let theta = std::f32::consts::TAU * (j as f32 + 0.5) / RAY_COUNT as f32; // clockwise from the top
    let from_bottom = (theta - std::f32::consts::PI).abs() / std::f32::consts::PI; // 0 at the bottom, 1 at the top
    let pos = from_bottom * (n - 1) as f32;
    let i = (pos.floor() as usize).min(n - 1);
    let k = (i + 1).min(n - 1);
    let f = pos - i as f32;
    values[i] * (1.0 - f) + values[k] * f
}

fn seg(out: &mut String, r0: f32, r1: f32, sin: f32, cos: f32) {
    use std::fmt::Write;
    let _ = write!(
        out,
        "M {:.1} {:.1} L {:.1} {:.1} ",
        500.0 + r0 * sin,
        500.0 - r0 * cos,
        500.0 + r1 * sin,
        500.0 - r1 * cos
    );
}

/// `bands` and `peaks` are 0..1, low frequencies first (same length).
pub fn build(bands: &[f32], peaks: &[f32]) -> RayPaths {
    let mut p = RayPaths::default();
    for j in 0..RAY_COUNT {
        let theta = std::f32::consts::TAU * (j as f32 + 0.5) / RAY_COUNT as f32;
        let (sin, cos) = theta.sin_cos();
        let v = level_at(bands, j).clamp(0.0, 1.0);
        // A slight curve: quiet parts stay small, loud parts open up.
        let len = MIN_LEN + v.powf(1.2) * R_SPAN;
        let end = R_INNER + len;
        seg(&mut p.halo, R_INNER, end, sin, cos);
        seg(&mut p.body, R_INNER, end, sin, cos);
        if len > MIN_LEN + 6.0 {
            seg(&mut p.tips, end - len * TIP_FRACTION, end, sin, cos);
        }
        let pk = level_at(peaks, j).clamp(0.0, 1.0);
        if pk > v + 0.04 {
            let r = R_INNER + MIN_LEN + pk.powf(1.2) * R_SPAN + 6.0;
            seg(&mut p.peaks, r, r + 3.5, sin, cos);
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_segments(s: &str) -> usize {
        s.matches('M').count()
    }

    #[test]
    fn quiet_draws_idle_ticks_only() {
        let z = [0.0_f32; 60];
        let p = build(&z, &z);
        assert_eq!(count_segments(&p.body), RAY_COUNT);
        assert_eq!(count_segments(&p.halo), RAY_COUNT);
        assert!(p.tips.is_empty() && p.peaks.is_empty());
    }

    #[test]
    fn loud_adds_tips_and_peaks() {
        let b = [1.0_f32; 60];
        let mut pk = [1.0_f32; 60];
        let p = build(&b, &pk);
        assert_eq!(count_segments(&p.tips), RAY_COUNT);
        assert!(p.peaks.is_empty(), "peak at the ray end needs no marker");
        pk = [1.0; 60];
        let half = [0.5_f32; 60];
        assert_eq!(count_segments(&build(&half, &pk).peaks), RAY_COUNT);
    }

    #[test]
    fn mirrored_and_lows_at_the_bottom() {
        let mut b = [0.0_f32; 60];
        b[0] = 1.0; // lowest band only
        for j in 0..RAY_COUNT / 2 {
            let l = level_at(&b, j);
            let r = level_at(&b, RAY_COUNT - 1 - j);
            assert!((l - r).abs() < 1e-5, "ray {j} is not mirrored");
        }
        // The two rays next to the bottom (j = 59, 60) carry the most, the top ones nothing.
        assert!(level_at(&b, 59) > level_at(&b, 30));
        assert_eq!(level_at(&b, 0), 0.0);
    }

    #[test]
    fn stays_inside_the_box() {
        let o = [1.0_f32; 60];
        let p = build(&o, &o);
        for n in p.body.split_whitespace().filter_map(|t| t.parse::<f32>().ok()) {
            assert!((0.0..=1000.0).contains(&n), "{n} is outside the box");
        }
    }
}
