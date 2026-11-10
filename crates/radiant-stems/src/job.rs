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

/// Runs the job and returns the written files. `base` = `Artist - Title`; every set becomes
/// `out_dir/<base> - <set label>.wav` (sets without a label, i.e. empty or all four stems, are skipped).
/// `progress` gets 0..1 over the whole job.
pub fn create_stems(
    input: &Path,
    model: &Path,
    out_dir: &Path,
    base: &str,
    sets: &[StemSet],
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(f32),
) -> Result<Vec<PathBuf>, StemError> {
    let backend = OrtBackend::load(model, default_threads())?;
    run(input, backend, out_dir, base, sets, cancel, progress)
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
    let base = safe_file_name(base);
    let wanted: Vec<(StemSet, String)> = sets.iter().filter_map(|s| s.file_label().map(|l| (*s, l))).collect();
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
    let stems = separate(&samples, &mut backend, cancel, &mut |done, total| {
        progress(DECODE_SHARE + NETWORK_SHARE * done as f32 / total as f32)
    })?;
    drop(samples);

    fs::create_dir_all(out_dir).map_err(|e| StemError::Io(format!("{}: {e}", out_dir.display())))?;
    let mut written = Vec::new();
    for (i, (set, label)) in wanted.iter().enumerate() {
        let path = out_dir.join(format!("{base} - {label}.wav"));
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
