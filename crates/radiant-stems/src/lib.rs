//! radiant-stems: AI stem separation with Demucs (htdemucs, 4 stems: drums, bass, other, vocals) via ONNX Runtime.
//!
//! DONE (stage 11, base): the contract of the network and the segment planner (`demucs`), the stems and the output
//! file names (`stem`), the model file manager: download with resume + SHA-256, import, delete (`store`), the
//! check for the ONNX Runtime library.
//! TODO: the ONNX Runtime wrapper that implements `demucs::Backend`, decode + resample to 44.1 kHz stereo, STFT /
//!       iSTFT (4096 / 1024), per-track normalization, overlap-add, 16-bit WAV writer, progress / cancel / worker
//!       thread (see PROGRESS.md, stage 11).
//!
//! Low-power mode (Settings -> Power): stem separation is the heaviest job of the app, so it must never start
//! (and no model may be downloaded) while the mode is on. Every entry point asks [`allowed`] first.

pub mod demucs;
pub mod stem;
pub mod store;

pub use stem::{Stem, StemSet};
pub use store::{hash_file, ModelSpec, ModelState, ModelStore, Progress, MODEL};

use std::fmt;
use std::path::PathBuf;

/// Stem separation may run only when low-power mode is off.
pub fn allowed(low_power: bool) -> bool {
    !low_power
}

/// Everything that can go wrong in this crate. The texts are for logs; the interface shows its own translated
/// messages chosen by the variant.
#[derive(Debug)]
pub enum StemError {
    /// The model's exact size / SHA-256 are not set in this build (`store::MODEL`), so nothing can be verified.
    NotPinned,
    /// The user cancelled.
    Cancelled,
    /// Disk error (the text names the path).
    Io(String),
    /// Network error.
    Http(String),
    /// The file has the wrong size or SHA-256 (it is not the expected model).
    Mismatch,
    /// The network loaded fine but is not the htdemucs 4-stem contract (see `demucs::check_io`).
    BadModel(String),
}

impl fmt::Display for StemError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StemError::NotPinned => write!(f, "the model size and checksum are not set in this build"),
            StemError::Cancelled => write!(f, "cancelled"),
            StemError::Io(e) | StemError::Http(e) | StemError::BadModel(e) => write!(f, "{e}"),
            StemError::Mismatch => write!(f, "the file does not match the expected size or checksum"),
        }
    }
}

impl std::error::Error for StemError {}

/// Name of the ONNX Runtime library that must lie next to the exe (the `ort` crate is used with `load-dynamic`,
/// so the runtime is not linked into the exe and the exe stays small).
#[cfg(windows)]
pub const RUNTIME_LIBRARY: &str = "onnxruntime.dll";
#[cfg(not(windows))]
pub const RUNTIME_LIBRARY: &str = "libonnxruntime.so";

/// Full path where the program looks for the runtime library (next to the exe).
pub fn runtime_library_path() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join(RUNTIME_LIBRARY))
}

/// The runtime library is there.
pub fn runtime_present() -> bool {
    runtime_library_path().map_or(false, |p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_are_not_allowed_in_low_power_mode() {
        assert!(allowed(false));
        assert!(!allowed(true));
    }

    #[test]
    fn errors_have_readable_texts() {
        assert!(!StemError::NotPinned.to_string().is_empty());
        assert_eq!(StemError::Io("disk full".into()).to_string(), "disk full");
    }
}
