//! Color math for the custom accent color of the palette settings. No UI here.

/// HSV (all three in 0.0..=1.0, hue wraps around) to 8-bit RGB.
pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (u8, u8, u8) {
    let h = if h.is_finite() { h.rem_euclid(1.0) } else { 0.0 };
    let s = if s.is_finite() { s.clamp(0.0, 1.0) } else { 0.0 };
    let v = if v.is_finite() { v.clamp(0.0, 1.0) } else { 0.0 };
    let sector = h * 6.0;
    let i = sector.floor() as i32 % 6;
    let f = sector - sector.floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let to8 = |x: f32| (x * 255.0).round().clamp(0.0, 255.0) as u8;
    (to8(r), to8(g), to8(b))
}

/// True when text on this color should be dark (the color is light: yellow, light gray...).
pub fn needs_dark_text(rgb: (u8, u8, u8)) -> bool {
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let lum = 0.2126 * lin(rgb.0) + 0.7152 * lin(rgb.1) + 0.0722 * lin(rgb.2);
    lum > 0.35
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_colors() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(1.0 / 3.0, 1.0, 1.0), (0, 255, 0));
        assert_eq!(hsv_to_rgb(2.0 / 3.0, 1.0, 1.0), (0, 0, 255));
    }

    #[test]
    fn gray_white_black() {
        assert_eq!(hsv_to_rgb(0.4, 0.0, 1.0), (255, 255, 255));
        assert_eq!(hsv_to_rgb(0.4, 0.7, 0.0), (0, 0, 0));
        assert_eq!(hsv_to_rgb(0.3, 0.0, 0.5), (128, 128, 128));
    }

    #[test]
    fn hue_wraps_and_bad_input_is_safe() {
        assert_eq!(hsv_to_rgb(1.0, 1.0, 1.0), (255, 0, 0));
        assert_eq!(hsv_to_rgb(f32::NAN, 2.0, -1.0), (0, 0, 0));
    }

    #[test]
    fn text_color_choice() {
        assert!(needs_dark_text((255, 255, 0)));
        assert!(needs_dark_text((255, 255, 255)));
        assert!(!needs_dark_text((0, 0, 255)));
        assert!(!needs_dark_text((139, 108, 255)));
    }
}
