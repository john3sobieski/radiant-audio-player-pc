//! Equalizer presets, ported 1:1 from Android `EqPresets.kt` (10 presets, 12 bands, dB).
//! Band order matches [`crate::EQ_BANDS_HZ`]: 32 Hz ... 16 kHz.
//! Names are English keys for now; translation comes with the localization step (README section 7).

/// A ready-made equalizer curve.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqPreset {
    /// Key of the Android string `preset_<key>` (used later to look up the translated name).
    pub key: &'static str,
    /// English display name.
    pub name: &'static str,
    /// Gains in dB for the 12 bands.
    pub gains: [f32; 12],
}

pub const EQ_PRESETS: [EqPreset; 10] = [
    EqPreset { key: "rock", name: "Rock", gains: [4.0, 3.0, 2.0, 1.0, -1.0, -2.0, 0.0, 2.0, 3.0, 4.0, 4.0, 3.0] },
    EqPreset { key: "pop", name: "Pop", gains: [-1.0, 0.0, 2.0, 3.0, 3.0, 2.0, 0.0, -1.0, -1.0, 0.0, 1.0, 1.0] },
    EqPreset { key: "hiphop", name: "Hip-Hop", gains: [5.0, 5.0, 4.0, 2.0, 1.0, -1.0, -1.0, 1.0, 1.0, 2.0, 2.0, 1.0] },
    EqPreset { key: "electronic", name: "Electronic", gains: [5.0, 4.0, 2.0, 0.0, -2.0, -1.0, 0.0, 1.0, 3.0, 4.0, 4.0, 3.0] },
    EqPreset { key: "jazz", name: "Jazz", gains: [3.0, 2.0, 1.0, 2.0, -2.0, -2.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0] },
    EqPreset { key: "classical", name: "Classical", gains: [2.0, 2.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0] },
    EqPreset { key: "metal", name: "Metal", gains: [5.0, 4.0, 2.0, 0.0, -3.0, -3.0, 0.0, 3.0, 5.0, 5.0, 4.0, 3.0] },
    EqPreset { key: "acoustic", name: "Acoustic", gains: [3.0, 3.0, 2.0, 1.0, 1.0, 0.0, 1.0, 2.0, 2.0, 3.0, 2.0, 1.0] },
    EqPreset { key: "vocal", name: "Vocal", gains: [-3.0, -2.0, -1.0, 1.0, 3.0, 4.0, 4.0, 3.0, 1.0, 0.0, -1.0, -2.0] },
    EqPreset { key: "bass", name: "Bass", gains: [8.0, 7.0, 5.0, 3.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0] },
];

/// Index of the preset whose curve equals `gains` exactly, if any (to highlight it in the UI).
pub fn matching_preset(gains: &[f32; 12]) -> Option<usize> {
    EQ_PRESETS.iter().position(|p| &p.gains == gains)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_gains_within_eq_range() {
        for p in &EQ_PRESETS {
            assert!(p.gains.iter().all(|g| g.abs() <= 12.0), "{} out of range", p.name);
        }
    }

    #[test]
    fn keys_are_unique() {
        for (i, a) in EQ_PRESETS.iter().enumerate() {
            for b in &EQ_PRESETS[i + 1..] {
                assert_ne!(a.key, b.key);
            }
        }
    }

    #[test]
    fn matching_finds_preset_and_rejects_custom() {
        assert_eq!(matching_preset(&EQ_PRESETS[6].gains), Some(6));
        assert_eq!(matching_preset(&[0.0; 12]), None);
    }
}
