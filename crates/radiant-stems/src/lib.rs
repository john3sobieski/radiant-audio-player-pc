//! radiant-stems: AI stem separation (Open-Unmix umxhq via ONNX).
//!
//! DONE: constants (model files, sizes, SHA-256 from README 5.3).
//! TODO: model store (download, verify, import by checksum), decode to 44.1 kHz stereo,
//!       STFT (N_FFT 4096, HOP 1024, centered/reflect), block processing (512 hops + 86 context),
//!       ONNX passes for all four stems, soft mask v^2/sum(v^2), 16-bit WAV writer,
//!       progress/cancel, runs in a worker thread. Keep this crate optional for the UI.
//!
//! Low-power mode (Settings -> Power): stem separation is the heaviest job of the app, so it must never start
//! (and no model may be downloaded) while the mode is on. Every entry point asks [`allowed`] first. The UI already
//! dims "Create stems" in the track menu and says why; wire this function in when the pipeline exists.

pub const N_FFT: usize = 4096;
pub const HOP: usize = 1024;
pub const BINS: usize = N_FFT / 2 + 1; // 2049
pub const CORE_HOPS: usize = 512;
pub const CONTEXT_HOPS: usize = 86;

/// Stem separation may run only when low-power mode is off.
pub fn allowed(low_power: bool) -> bool {
    !low_power
}

pub struct StemModel {
    pub name: &'static str,
    pub hf_repo: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
}

impl StemModel {
    pub fn url(&self) -> String {
        format!("https://huggingface.co/{}/resolve/main/model.onnx", self.hf_repo)
    }
}

pub const MODELS: [StemModel; 4] = [
    StemModel {
        name: "vocals",
        hf_repo: "MixDirective/open-unmix-umxhq-vocals-onnx",
        bytes: 35_623_512,
        sha256: "f27742bb52b24cb039614dc76d0764e279df3eb2a9d6daa82c6daee3369ec747",
    },
    StemModel {
        name: "drums",
        hf_repo: "MixDirective/open-unmix-umxhq-drums-onnx",
        bytes: 35_623_507,
        sha256: "471628e1e48138a634f962a89f622929980cf5485ed0a4e6e648e9dfcd1145d9",
    },
    StemModel {
        name: "bass",
        hf_repo: "MixDirective/open-unmix-umxhq-bass-onnx",
        bytes: 35_623_502,
        sha256: "8931033f04666fdcf300d610579351d74275a6c55dd4346009290c844033726f",
    },
    StemModel {
        name: "other",
        hf_repo: "MixDirective/open-unmix-umxhq-other-onnx",
        bytes: 35_623_507,
        sha256: "fe62c1d8fb8326c7e68540a675dfa186a2b131f763fb41050ea5e9a9859eb76e",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stems_are_not_allowed_in_low_power_mode() {
        assert!(allowed(false));
        assert!(!allowed(true));
    }

    #[test]
    fn bins_match_model_input() {
        assert_eq!(BINS, 2049);
        assert_eq!(MODELS[0].url(), "https://huggingface.co/MixDirective/open-unmix-umxhq-vocals-onnx/resolve/main/model.onnx");
    }
}
