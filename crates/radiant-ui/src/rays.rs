//! Geometry of the spectrum rays around the progress ring ("sun rays").
//!
//! Pure functions: spectrum bands in, SVG path strings out (one filled `Path` per layer in the
//! Slint file). Coordinates are in a 1000 x 1000 box that Slint scales to the real size.
//! The ring has radius `RING_R`. Every ray is a thin pointed triangle; its length follows one band
//! of the spectrum, but angle, length range and band assignment differ ray by ray (fixed pseudo-random
//! numbers), the left and right half map the spectrum differently and `phase` slowly turns the whole
//! picture. So it looks like glowing sun rays, not like an equalizer.

use std::f32::consts::TAU;

pub const RAY_COUNT: usize = 168;
/// Ring radius in box units; the Slint file sizes the box to `ring-d * 1.4` to match.
pub const RING_R: f32 = 357.0;
const R_INNER: f32 = RING_R + 14.0;
/// Longest possible ray (box edge is at radius 500).
const R_SPAN: f32 = 122.0;
/// Idle length: a quiet ring keeps a fine fringe.
const MIN_LEN: f32 = 4.0;

#[derive(Default, Clone, PartialEq)]
pub struct RayPaths {
    /// Wide, faint triangles (soft glow around every ray).
    pub halo: String,
    /// The rays.
    pub body: String,
    /// Thin bright core of each ray.
    pub tips: String,
    /// Tiny sparks above the rays that fall back slowly.
    pub peaks: String,
}

/// Deterministic pseudo-random number in 0..1 for (ray, salt).
fn rnd(j: usize, salt: u32) -> f32 {
    let mut x = (j as u32).wrapping_mul(0x9E37_79B1) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    x = x.wrapping_mul(0x297A_2D39);
    x ^= x >> 15;
    (x >> 8) as f32 / (1u32 << 24) as f32
}

/// Value of a band curve at a fractional position (linear interpolation).
fn sample(values: &[f32], pos: f32) -> f32 {
    let n = values.len();
    if n == 0 {
        return 0.0;
    }
    let pos = pos.clamp(0.0, (n - 1) as f32);
    let i = pos.floor() as usize;
    let k = (i + 1).min(n - 1);
    let f = pos - i as f32;
    values[i] * (1.0 - f) + values[k] * f
}

/// Where on the spectrum ray `j` listens (0..n-1) for the given turn `phase` (in turns).
/// Lows sit at one point of the circle and highs on the opposite side, but the two halves bend the
/// scale differently and every ray is shifted a little, so the picture is not mirrored.
fn band_pos(j: usize, theta: f32, phase: f32, n: usize) -> f32 {
    let u = (theta / TAU + phase).rem_euclid(1.0);
    let t = if u < 0.5 { u * 2.0 } else { (1.0 - u) * 2.0 }; // 0 .. 1 .. 0
    let warp = if u < 0.5 { t.powf(0.85) } else { t.powf(1.25) };
    let jitter = (rnd(j, 3) - 0.5) * 3.0;
    warp * (n - 1) as f32 + jitter
}

fn tri(out: &mut String, sin: f32, cos: f32, r0: f32, r1: f32, half_w: f32) {
    use std::fmt::Write;
    // Base corners are shifted sideways (perpendicular to the ray), the tip sits on the ray axis.
    let (px, py) = (cos, sin);
    let (bx, by) = (500.0 + r0 * sin, 500.0 - r0 * cos);
    let _ = write!(
        out,
        "M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} Z ",
        bx - px * half_w,
        by - py * half_w,
        500.0 + r1 * sin,
        500.0 - r1 * cos,
        bx + px * half_w,
        by + py * half_w
    );
}

/// `bands` and `peaks` are 0..1, low frequencies first. `phase` is the slow rotation in turns.
pub fn build(bands: &[f32], peaks: &[f32], phase: f32) -> RayPaths {
    let n = bands.len().max(1);
    let mut p = RayPaths::default();
    for j in 0..RAY_COUNT {
        let pitch = TAU / RAY_COUNT as f32;
        // Angle with a random shift of up to +-45 % of the pitch (clumps and gaps, like real light).
        let theta = pitch * (j as f32 + 0.5 + (rnd(j, 1) - 0.5) * 0.9);
        let (sin, cos) = theta.sin_cos();

        let pos = band_pos(j, theta, phase, n);
        // Every ray has its own sensitivity; a few are "long" rays that reach much further.
        let gain = 0.55 + 0.45 * rnd(j, 2);
        let long = if rnd(j, 4) > 0.82 { 1.0 } else { 0.62 + 0.3 * rnd(j, 5) };
        let v = (sample(bands, pos) * gain).clamp(0.0, 1.0);
        let len = MIN_LEN + v.powf(1.25) * R_SPAN * long;
        let end = R_INNER + len;
        // Thin: width follows the length a little (long rays are a bit broader at the root).
        let w = 0.9 + 1.5 * (len / R_SPAN).min(1.0) + 0.5 * rnd(j, 6);

        tri(&mut p.halo, sin, cos, R_INNER - 2.0, end + 4.0, w * 3.2);
        tri(&mut p.body, sin, cos, R_INNER, end, w);
        if len > MIN_LEN + 7.0 {
            tri(&mut p.tips, sin, cos, R_INNER + len * 0.15, end - len * 0.04, w * 0.42);
        }
        let pk = (sample(peaks, pos) * gain).clamp(0.0, 1.0);
        let peak_len = MIN_LEN + pk.powf(1.25) * R_SPAN * long;
        if peak_len > len + 6.0 {
            let r = R_INNER + peak_len + 5.0;
            tri(&mut p.peaks, sin, cos, r, r + 4.0, 0.9);
        }
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangles(s: &str) -> usize {
        s.matches('M').count()
    }

    #[test]
    fn quiet_draws_a_fine_fringe_only() {
        let z = [0.0_f32; 60];
        let p = build(&z, &z, 0.0);
        assert_eq!(triangles(&p.body), RAY_COUNT);
        assert_eq!(triangles(&p.halo), RAY_COUNT);
        assert!(p.tips.is_empty() && p.peaks.is_empty());
    }

    #[test]
    fn loud_adds_cores_and_sparks() {
        let b = [0.6_f32; 60];
        let pk = [1.0_f32; 60];
        let p = build(&b, &pk, 0.0);
        assert!(triangles(&p.tips) > RAY_COUNT / 2);
        assert!(triangles(&p.peaks) > RAY_COUNT / 4);
    }

    #[test]
    fn not_mirrored_and_turns_with_phase() {
        let mut b = [0.0_f32; 60];
        for (i, v) in b.iter_mut().enumerate() {
            *v = i as f32 / 59.0;
        }
        let a = build(&b, &b, 0.0).body;
        assert_ne!(a, build(&b, &b, 0.25).body, "phase must change the picture");
        // Left and right half must not be copies of each other: compare total lengths.
        let len_of = |from: usize, to: usize| -> f32 {
            (from..to)
                .map(|j| {
                    let theta = TAU * (j as f32 + 0.5) / RAY_COUNT as f32;
                    sample(&b, band_pos(j, theta, 0.0, 60)) * (0.55 + 0.45 * rnd(j, 2))
                })
                .sum()
        };
        let (r, l) = (len_of(0, RAY_COUNT / 2), len_of(RAY_COUNT / 2, RAY_COUNT));
        assert!((r - l).abs() > 0.5, "halves look identical: {r} vs {l}");
    }

    #[test]
    fn follows_the_spectrum() {
        // Louder spectrum -> more ink in the body layer (longer rays).
        let quiet = build(&[0.1; 60], &[0.1; 60], 0.0);
        let loud = build(&[0.9; 60], &[0.9; 60], 0.0);
        let reach = |s: &str| {
            s.split_whitespace()
                .filter_map(|t| t.parse::<f32>().ok())
                .map(|v| (v - 500.0).abs())
                .fold(0.0_f32, f32::max)
        };
        assert!(reach(&loud.body) > reach(&quiet.body) + 40.0);
    }

    #[test]
    fn stays_inside_the_box() {
        let o = [1.0_f32; 60];
        for phase in [0.0, 0.3, 0.77] {
            let p = build(&o, &o, phase);
            for layer in [&p.halo, &p.body, &p.tips, &p.peaks] {
                for n in layer.split_whitespace().filter_map(|t| t.parse::<f32>().ok()) {
                    assert!((0.0..=1000.0).contains(&n), "{n} is outside the box");
                }
            }
        }
    }

    #[test]
    fn rnd_is_deterministic_and_spread() {
        assert_eq!(rnd(5, 2), rnd(5, 2));
        let mean: f32 = (0..1000).map(|j| rnd(j, 1)).sum::<f32>() / 1000.0;
        assert!((mean - 0.5).abs() < 0.05, "mean {mean}");
        assert!(std::f32::consts::PI > 3.0);
    }
}
