//! Small response-curve drawing for the equalizer: 12 gains in dB -> smooth SVG path strings.
//!
//! Pure functions. The box is `W` x `H` units (the Slint file scales it to the real size); the zero line
//! is at `MID`, +-12 dB reach `MID -+ AMP`. The top strip (`0..CURVE_TOP`) is left free for the caption.

pub const W: f32 = 328.0;
pub const H: f32 = 46.0;
/// Height of the zero line.
pub const MID: f32 = 29.0;
/// Units per +-12 dB.
pub const AMP: f32 = 11.0;
const PAD: f32 = 8.0;
const CURVE_TOP: f32 = 15.0;

/// Position of band `i` (0..12) in box units.
pub fn point(i: usize, gain_db: f32) -> (f32, f32) {
    let x = PAD + (W - 2.0 * PAD) * i as f32 / 11.0;
    let y = MID - gain_db.clamp(-12.0, 12.0) / 12.0 * AMP;
    (x, y)
}

/// (line, fill): a smooth line through the 12 points (Catmull-Rom turned into cubic Beziers) and the same
/// line closed down to the bottom edge for a soft fill.
pub fn paths(gains: &[f32; 12]) -> (String, String) {
    use std::fmt::Write;
    let pts: Vec<(f32, f32)> = gains.iter().enumerate().map(|(i, g)| point(i, *g)).collect();
    let at = |i: isize| pts[i.clamp(0, 11) as usize];
    let mut line = String::new();
    let _ = write!(line, "M {:.1} {:.1}", pts[0].0, pts[0].1);
    for i in 0..11isize {
        let (p0, p1, p2, p3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
        // Control points stay inside the drawing area, so a steep step does not overshoot the box.
        let c1 = (p1.0 + (p2.0 - p0.0) / 6.0, (p1.1 + (p2.1 - p0.1) / 6.0).clamp(CURVE_TOP, H - 2.0));
        let c2 = (p2.0 - (p3.0 - p1.0) / 6.0, (p2.1 - (p3.1 - p1.1) / 6.0).clamp(CURVE_TOP, H - 2.0));
        let _ = write!(line, " C {:.1} {:.1} {:.1} {:.1} {:.1} {:.1}", c1.0, c1.1, c2.0, c2.1, p2.0, p2.1);
    }
    let mut fill = line.clone();
    let _ = write!(fill, " L {:.1} {:.1} L {:.1} {:.1} Z", pts[11].0, H, pts[0].0, H);
    (line, fill)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(s: &str) -> Vec<f32> {
        s.split_whitespace().filter_map(|t| t.parse::<f32>().ok()).collect()
    }

    #[test]
    fn flat_curve_runs_along_the_zero_line() {
        let (line, _) = paths(&[0.0; 12]);
        let n = numbers(&line);
        assert_eq!(line.matches('C').count(), 11);
        for pair in n.chunks(2) {
            assert!((pair[1] - MID).abs() < 0.11, "y = {}", pair[1]);
        }
    }

    #[test]
    fn boost_goes_up_and_cut_goes_down() {
        assert!(point(3, 12.0).1 < MID);
        assert!(point(3, -12.0).1 > MID);
        assert!((point(0, 0.0).0 - PAD).abs() < 1e-3);
        assert!((point(11, 0.0).0 - (W - PAD)).abs() < 1e-3);
    }

    #[test]
    fn everything_stays_inside_the_box() {
        let mut g = [0.0_f32; 12];
        for (i, v) in g.iter_mut().enumerate() {
            *v = if i % 2 == 0 { 12.0 } else { -12.0 };
        }
        let (line, fill) = paths(&g);
        for s in [&line, &fill] {
            let n = numbers(s);
            for pair in n.chunks(2) {
                assert!((0.0..=W).contains(&pair[0]), "x = {}", pair[0]);
                assert!((0.0..=H).contains(&pair[1]), "y = {}", pair[1]);
            }
        }
        assert!(fill.ends_with('Z'));
    }

    #[test]
    fn out_of_range_gains_are_clamped() {
        assert_eq!(point(5, 99.0), point(5, 12.0));
        assert_eq!(point(5, -99.0), point(5, -12.0));
    }
}
