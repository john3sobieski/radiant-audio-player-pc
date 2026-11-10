//! The four Demucs stems, sets of them, and the names of the output files.

/// One stem. The order is the order of the Demucs output (`drums, bass, other, vocals`): `index()` is the position
/// in the network output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stem {
    Drums,
    Bass,
    Other,
    Vocals,
}

impl Stem {
    /// In network order.
    pub const ALL: [Stem; 4] = [Stem::Drums, Stem::Bass, Stem::Other, Stem::Vocals];

    /// Position in the network output.
    pub fn index(self) -> usize {
        match self {
            Stem::Drums => 0,
            Stem::Bass => 1,
            Stem::Other => 2,
            Stem::Vocals => 3,
        }
    }

    /// Name used in file names (English on purpose: the name depends only on the selection, never on the UI language,
    /// so a repeated run overwrites the same file).
    pub fn label(self) -> &'static str {
        match self {
            Stem::Drums => "Drums",
            Stem::Bass => "Bass",
            Stem::Other => "Other",
            Stem::Vocals => "Vocals",
        }
    }
}

/// A selection of stems that is mixed into one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct StemSet(u8);

impl StemSet {
    pub const EMPTY: StemSet = StemSet(0);
    pub const ALL: StemSet = StemSet(0b1111);

    pub fn with(self, stem: Stem) -> StemSet {
        StemSet(self.0 | 1 << stem.index())
    }

    pub fn without(self, stem: Stem) -> StemSet {
        StemSet(self.0 & !(1 << stem.index()))
    }

    pub fn contains(self, stem: Stem) -> bool {
        self.0 & (1 << stem.index()) != 0
    }

    pub fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn from_stems(stems: &[Stem]) -> StemSet {
        stems.iter().fold(StemSet::EMPTY, |set, s| set.with(*s))
    }

    /// From four on/off flags (the checkboxes of the Stems tab).
    pub fn from_flags(vocals: bool, drums: bool, bass: bool, other: bool) -> StemSet {
        [(Stem::Vocals, vocals), (Stem::Drums, drums), (Stem::Bass, bass), (Stem::Other, other)]
            .into_iter()
            .filter(|(_, on)| *on)
            .fold(StemSet::EMPTY, |set, (s, _)| set.with(s))
    }

    /// Members in the display order of the file names: Vocals, Drums, Bass, Other.
    pub fn members(self) -> Vec<Stem> {
        [Stem::Vocals, Stem::Drums, Stem::Bass, Stem::Other].into_iter().filter(|s| self.contains(*s)).collect()
    }

    /// The part of the file name that depends on the selection: `Vocals`, `Vocals + Drums`, `No Bass` (all but one).
    /// `None` for an empty selection and for all four stems (that is just the original track).
    pub fn file_label(self) -> Option<String> {
        match self.len() {
            0 | 4 => None,
            3 => {
                let missing = Stem::ALL.into_iter().find(|s| !self.contains(*s))?;
                Some(format!("No {}", missing.label()))
            }
            _ => Some(self.members().iter().map(|s| s.label()).collect::<Vec<_>>().join(" + ")),
        }
    }
}

/// The files one run makes: the chosen mix (when it has a name, i.e. 1..=3 parts) and, with `each_part`, every part
/// on its own too. The mix comes first. Empty when the choice gives no file at all.
pub fn output_sets(selection: StemSet, each_part: bool) -> Vec<StemSet> {
    let mut sets = Vec::new();
    if selection.file_label().is_some() {
        sets.push(selection);
    }
    if each_part {
        for stem in [Stem::Vocals, Stem::Drums, Stem::Bass, Stem::Other] {
            let single = StemSet::EMPTY.with(stem);
            if !sets.contains(&single) {
                sets.push(single);
            }
        }
    }
    sets
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn network_order_is_drums_bass_other_vocals() {
        let order: Vec<_> = Stem::ALL.iter().map(|s| s.index()).collect();
        assert_eq!(order, [0, 1, 2, 3]);
        assert_eq!(Stem::Vocals.index(), 3);
    }

    #[test]
    fn file_labels() {
        assert_eq!(StemSet::from_stems(&[Stem::Vocals]).file_label().unwrap(), "Vocals");
        assert_eq!(StemSet::from_stems(&[Stem::Drums, Stem::Vocals]).file_label().unwrap(), "Vocals + Drums");
        assert_eq!(StemSet::ALL.without(Stem::Bass).file_label().unwrap(), "No Bass");
        assert_eq!(StemSet::ALL.without(Stem::Vocals).file_label().unwrap(), "No Vocals");
        assert_eq!(StemSet::EMPTY.file_label(), None);
        assert_eq!(StemSet::ALL.file_label(), None);
    }

    #[test]
    fn set_operations() {
        let s = StemSet::EMPTY.with(Stem::Bass).with(Stem::Other);
        assert!(s.contains(Stem::Bass) && !s.contains(Stem::Drums));
        assert_eq!(s.len(), 2);
        assert!(!s.without(Stem::Bass).contains(Stem::Bass));
        assert!(StemSet::EMPTY.is_empty());
    }

    #[test]
    fn flags_make_sets() {
        assert_eq!(StemSet::from_flags(false, true, true, true), StemSet::ALL.without(Stem::Vocals));
        assert_eq!(StemSet::from_flags(false, false, false, false), StemSet::EMPTY);
    }

    #[test]
    fn output_sets_rules() {
        let vocals = StemSet::EMPTY.with(Stem::Vocals);
        let karaoke = StemSet::ALL.without(Stem::Vocals);
        // One mix.
        assert_eq!(output_sets(karaoke, false), vec![karaoke]);
        // Nothing / all four alone give no file; all four with "each part" give the four parts.
        assert!(output_sets(StemSet::EMPTY, false).is_empty());
        assert!(output_sets(StemSet::ALL, false).is_empty());
        assert_eq!(output_sets(StemSet::ALL, true).len(), 4);
        // Mix + four parts; a single part is not made twice.
        let both = output_sets(karaoke, true);
        assert_eq!(both.len(), 5);
        assert_eq!(both[0], karaoke);
        assert_eq!(output_sets(vocals, true).len(), 4);
        assert_eq!(output_sets(vocals, true)[0], vocals);
    }
}
