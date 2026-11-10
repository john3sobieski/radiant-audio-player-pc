//! One "Create stems" job: file -> decode -> separate -> WAV files. This is what the interface calls (on a worker
//! thread); everything below it is testable without a model.

use crate::demucs::{Backend, SAMPLE_RATE};
use crate::ort_backend::OrtBackend;
use crate::separate::separate;
use crate::stem::StemSet;
use crate::wav::write_stereo16;
use crate::StemError;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

/// Share of the progress bar: decoding, the network, writing the files.
const DECODE_SHARE: f32 = 0.05;
const NETWORK_SHARE: f32 = 0.87;

/// File-name part from a title: characters Windows forbids become `_`, no trailing dots / spaces, never empty.
pub fn safe_file_name(name: &str) -> String {
    let cleaned: String =
        name.chars().map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c }).collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim_end().to_string();
    if cleaned.is_empty() {
        "Track".into()
    } else {
        cleaned
    }
}

/// Intra-op threads for the network: half of the cores, so the player and the interface stay smooth.
pub fn default_threads() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get() / 2).clamp(1, 8)
}

/// File name of a set: `<base> - <set label>.wav` (`base` = `Artist - Title`). `None` for sets without a label
/// (empty, or all four stems: that is just the original track).
pub fn file_name(base: &str, set: StemSet) -> Option<String> {
    Some(format!("{} - {}.wav", safe_file_name(base), set.file_label()?))
}

/// The part of a stem file name after `<base> - `: one label of [`StemSet::file_label`] (`Vocals`, `Vocals + Drums`,
/// `No Bass`).
fn is_stem_label(label: &str) -> bool {
    let known = |s: &str| StemSet::ALL.members().iter().any(|m| m.label() == s);
    match label.strip_prefix("No ") {
        Some(rest) => known(rest),
        None => !label.is_empty() && label.split(" + ").all(known),
    }
}

/// The file is itself a stem made by this program: it lies in `<...>/Stems/<Artist - Title>/` (where the jobs write),
/// or (moved elsewhere) it is named `<folder name> - <label>` like our files are. Stems are not separated again:
/// the result would only be worse (and the original track is the better input).
pub fn is_stem_file(path: &Path) -> bool {
    let folder = path.parent();
    let in_stems_folder = folder
        .and_then(Path::parent)
        .and_then(Path::file_name)
        .map_or(false, |n| n.eq_ignore_ascii_case("Stems"));
    if in_stems_folder {
        return true;
    }
    let (Some(folder_name), Some(file_stem)) =
        (folder.and_then(Path::file_name).and_then(|n| n.to_str()), path.file_stem().and_then(|n| n.to_str()))
    else {
        return false;
    };
    file_stem
        .strip_prefix(folder_name)
        .and_then(|rest| rest.strip_prefix(" - "))
        .map_or(false, is_stem_label)
}

/// What a finished job reports.
#[derive(Debug)]
pub struct Done {
    /// The written files, in the order of the sets.
    pub files: Vec<PathBuf>,
    /// The network ran on the GPU (false = CPU, also when the GPU was asked for but could not be used).
    pub gpu: bool,
}

/// Runs the job. `base` = `Artist - Title`; every set becomes `out_dir/<file_name(base, set)>` (sets without a
/// label are skipped). `gpu` asks for DirectML (falls back to the CPU, see `OrtBackend::load`).
/// `progress` gets 0..1 over the whole job.
pub fn create_stems(
    input: &Path,
    model: &Path,
    out_dir: &Path,
    base: &str,
    sets: &[StemSet],
    gpu: bool,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<Done, StemError> {
    let mut backend = OrtBackend::load(model, default_threads(), gpu)?;
    let files = run_with(input, &mut backend, out_dir, base, sets, cancel, progress)?;
    // Asked after the job: a GPU that returned garbage is dropped in the middle of the run (see `OrtBackend::run`).
    Ok(Done { files, gpu: backend.using_gpu() })
}

/// The same with any network (the tests use a fake one).
pub fn run(
    input: &Path,
    mut backend: impl Backend,
    out_dir: &Path,
    base: &str,
    sets: &[StemSet],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<Vec<PathBuf>, StemError> {
    run_with(input, &mut backend, out_dir, base, sets, cancel, progress)
}

fn run_with(
    input: &Path,
    backend: &mut dyn Backend,
    out_dir: &Path,
    base: &str,
    sets: &[StemSet],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<Vec<PathBuf>, StemError> {
    let wanted: Vec<(StemSet, String)> = sets.iter().filter_map(|s| file_name(base, *s).map(|n| (*s, n))).collect();
    if wanted.is_empty() {
        return Ok(Vec::new());
    }
    progress(0.0);
    let samples = radiant_audio::decode_stereo(input, SAMPLE_RATE, cancel, &mut |p| progress(p * DECODE_SHARE))
        .map_err(|e| StemError::Decode(format!("{}: {e}", input.display())))?
        .ok_or(StemError::Cancelled)?;
    if samples.is_empty() {
        return Err(StemError::Decode(format!("{}: no audio", input.display())));
    }
    let stems = separate(&samples, backend, cancel, &mut |done, total| {
        progress(DECODE_SHARE + NETWORK_SHARE * done as f32 / total as f32)
    })?;
    drop(samples);

    fs::create_dir_all(out_dir).map_err(|e| StemError::Io(format!("{}: {e}", out_dir.display())))?;
    let mut written = Vec::new();
    for (i, (set, name)) in wanted.iter().enumerate() {
        let path = out_dir.join(name);
        write_stereo16(&path, SAMPLE_RATE, stems.frames(), cancel, |start, block| stems.mix_block(*set, start, block))?;
        written.push(path);
        progress(DECODE_SHARE + NETWORK_SHARE + (1.0 - DECODE_SHARE - NETWORK_SHARE) * (i + 1) as f32 / wanted.len() as f32);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_safe() {
        assert_eq!(safe_file_name("AC/DC - Who Made Who?"), "AC_DC - Who Made Who_");
        assert_eq!(safe_file_name("  Title...  "), "Title");
        assert_eq!(safe_file_name("???"), "___");
        assert_eq!(safe_file_name(" . "), "Track");
        assert_eq!(safe_file_name("Яндекс — Песня"), "Яндекс — Песня");
    }

    #[test]
    fn file_names_of_sets() {
        use crate::stem::Stem;
        let karaoke = StemSet::ALL.without(Stem::Vocals);
        assert_eq!(file_name("A - B", karaoke).as_deref(), Some("A - B - No Vocals.wav"));
        assert_eq!(file_name("A/B", StemSet::EMPTY.with(Stem::Bass)).as_deref(), Some("A_B - Bass.wav"));
        assert_eq!(file_name("A - B", StemSet::ALL), None);
        assert_eq!(file_name("A - B", StemSet::EMPTY), None);
    }

    #[test]
    fn our_own_stems_are_recognized() {
        // The layout the jobs write: <music>/Stems/<base>/<base> - <label>.wav
        assert!(is_stem_file(Path::new("D:/Music/Stems/A - B/A - B - Vocals + Drums.wav")));
        assert!(is_stem_file(Path::new("D:/Music/stems/A - B/anything.wav")));
        // Moved out of the Stems folder: the name still says so.
        assert!(is_stem_file(Path::new("D:/Elsewhere/A - B/A - B - No Bass.wav")));
        assert!(is_stem_file(Path::new("D:/Elsewhere/A - B/A - B - Other.wav")));
        // Ordinary tracks, also ones that merely look similar.
        assert!(!is_stem_file(Path::new("D:/Music/Album/01 Song.flac")));
        assert!(!is_stem_file(Path::new("D:/Music/A - B/A - B - Bass Line.mp3")));
        assert!(!is_stem_file(Path::new("D:/Music/A - B/Other - Vocals.mp3")));
        assert!(!is_stem_file(Path::new("D:/Music/Stems/loose.wav")));
    }

    #[test]
    fn threads_are_sane() {
        assert!((1..=8).contains(&default_threads()));
    }

    #[test]
    fn nothing_to_write_is_not_an_error() {
        struct Never;
        impl Backend for Never {
            fn run(&mut self, _: &[f32], _: &[f32]) -> Result<crate::demucs::SegmentOut, StemError> {
                unreachable!()
            }
        }
        let r = run(Path::new("none.wav"), Never, Path::new("."), "x", &[StemSet::EMPTY, StemSet::ALL], &AtomicBool::new(false), &mut |_| {});
        assert_eq!(r.unwrap(), Vec::<PathBuf>::new());
    }
}
