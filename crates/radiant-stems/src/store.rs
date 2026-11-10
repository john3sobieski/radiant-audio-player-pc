//! The model file on disk: where it lives, one-time download (resumable, SHA-256 verified), import from a file
//! the user already has, delete. Nothing here touches the network unless [`ModelStore::download`] is called, and
//! nothing is ever uploaded.
//!
//! The model is NOT part of the exe. To use a model the program must know its exact size and SHA-256
//! ([`ModelSpec::is_pinned`]); without them download and import refuse to work, because an unverified 100 MB
//! file from the internet must never be fed to the program. To pin a model, fill `bytes` and `sha256` of [`MODEL`]
//! (see the comment there).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::StemError;

/// A model the program can use.
pub struct ModelSpec {
    pub id: &'static str,
    /// File name inside the models folder.
    pub file_name: &'static str,
    /// HTTPS address of the file.
    pub url: &'static str,
    /// Exact size in bytes. `None` = not pinned yet.
    pub bytes: Option<u64>,
    /// Lower-case hex SHA-256 of the file. `None` = not pinned yet.
    pub sha256: Option<&'static str>,
    /// Rough size for the interface until the exact one is known.
    pub approx_bytes: u64,
}

impl ModelSpec {
    /// Size and checksum are known, so a file can be verified.
    pub fn is_pinned(&self) -> bool {
        self.bytes.is_some() && self.sha256.is_some()
    }
}

/// The htdemucs 4-stem network (MIT, Meta AI: facebookresearch/demucs), exported to ONNX with the STFT outside
/// (`mix [1,2,343980]`, `spec [1,4,2048,336]` -> `wave [1,4,2,343980]`, `spec_out [1,4,4,2048,336]`, fp16 weights
/// stored, cast to fp32 on load, 7.8 s segment).
///
/// PINNED by the owner from the downloaded file (size and `certutil -hashfile` SHA-256). To pin another file:
/// 1. The repo's `catalog.json` lists size and SHA-256 of `htdemucs.onnx`; or download the file once and run
///    `certutil -hashfile htdemucs.onnx SHA256` (Windows) and read the size in bytes.
/// 2. Put both here: `bytes: Some(<size>)`, `sha256: Some("<64 hex chars, lower case>")`.
/// 3. Keep a copy of the file you hashed: if the repo owner replaces the file the download stops matching and the
///    program reports a checksum error instead of using it.
/// If another export is chosen later, only this constant changes (and `check_io` must accept its tensors).
pub const MODEL: ModelSpec = ModelSpec {
    id: "htdemucs",
    file_name: "htdemucs.onnx",
    url: "https://huggingface.co/Carnifis/stemstudio-models/resolve/main/htdemucs.onnx",
    bytes: Some(90_561_288),
    sha256: Some("d10035a3bf94ff4a97cc17df99e22093865454da42c7828ed0a4bf9a6d13ab14"),
    approx_bytes: 90_561_288,
};

/// Progress of a long step. `total` is the full size in bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    Download { done: u64, total: u64 },
    Verify { done: u64, total: u64 },
}

/// What is on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelState {
    Missing,
    /// The file is there and has the pinned size (a full checksum check is done only on download / import).
    Ready,
    /// A file with this name exists but its size is wrong (damaged or replaced).
    WrongSize,
}

/// The models folder.
pub struct ModelStore {
    dir: PathBuf,
}

fn io_err(path: &Path, e: std::io::Error) -> StemError {
    StemError::Io(format!("{}: {e}", path.display()))
}

fn hex(digest: impl std::fmt::LowerHex) -> String {
    format!("{digest:x}")
}

/// SHA-256 of a file as lower-case hex, read in 1 MiB steps. `on_progress(done, total)` is called after every step.
pub fn hash_file(path: &Path, cancel: &AtomicBool, mut on_progress: impl FnMut(u64, u64)) -> Result<String, StemError> {
    let mut file = File::open(path).map_err(|e| io_err(path, e))?;
    let total = file.metadata().map_err(|e| io_err(path, e))?.len();
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut done = 0u64;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(StemError::Cancelled);
        }
        let n = file.read(&mut buf).map_err(|e| io_err(path, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        on_progress(done, total);
    }
    Ok(hex(hasher.finalize()))
}

/// Moves `from` over `to` (Windows cannot rename over an existing file).
fn replace_file(from: &Path, to: &Path) -> Result<(), StemError> {
    if to.exists() {
        fs::remove_file(to).map_err(|e| io_err(to, e))?;
    }
    fs::rename(from, to).map_err(|e| io_err(to, e))
}

impl ModelStore {
    pub fn new(dir: impl Into<PathBuf>) -> ModelStore {
        ModelStore { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Where the finished model file is (or will be).
    pub fn model_path(&self) -> PathBuf {
        self.dir.join(MODEL.file_name)
    }

    fn part_path(&self) -> PathBuf {
        self.dir.join(format!("{}.part", MODEL.file_name))
    }

    /// Quick look at the disk (no hashing).
    pub fn state(&self) -> ModelState {
        match fs::metadata(self.model_path()) {
            Err(_) => ModelState::Missing,
            Ok(m) => match MODEL.bytes {
                Some(wanted) if m.len() != wanted => ModelState::WrongSize,
                _ => ModelState::Ready,
            },
        }
    }

    fn pinned(&self) -> Result<(u64, &'static str), StemError> {
        match (MODEL.bytes, MODEL.sha256) {
            (Some(bytes), Some(sha)) => Ok((bytes, sha)),
            _ => Err(StemError::NotPinned),
        }
    }

    /// Downloads the model. A broken connection or a cancel keeps the half-loaded `.part` file and the next call
    /// continues from where it stopped. At the end the whole file is checked against the pinned SHA-256; on a mismatch
    /// the file is deleted and [`StemError::Mismatch`] is returned. `progress` is called often; throttle on the caller side.
    pub fn download(&self, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<(), StemError> {
        let (total, sha) = self.pinned()?;
        fs::create_dir_all(&self.dir).map_err(|e| io_err(&self.dir, e))?;
        let part = self.part_path();

        let mut have = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if have > total {
            fs::remove_file(&part).map_err(|e| io_err(&part, e))?;
            have = 0;
        }
        if have < total {
            self.fetch(&part, have, total, cancel, progress)?;
        }
        let size = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
        if size != total {
            return Err(StemError::Http(format!("the download stopped early ({size} of {total} bytes)")));
        }

        let digest = hash_file(&part, cancel, |done, total| progress(Progress::Verify { done, total }))?;
        if !digest.eq_ignore_ascii_case(sha) {
            let _ = fs::remove_file(&part);
            return Err(StemError::Mismatch);
        }
        replace_file(&part, &self.model_path())
    }

    /// One HTTP request (with `Range` when continuing) and the copy into the `.part` file.
    fn fetch(
        &self,
        part: &Path,
        mut have: u64,
        total: u64,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<(), StemError> {
        let agent = ureq::AgentBuilder::new()
            .https_only(true)
            .redirects(8)
            .timeout_connect(Duration::from_secs(15))
            .timeout_read(Duration::from_secs(30))
            .build();
        let response = loop {
            let mut request = agent.get(MODEL.url);
            if have > 0 {
                request = request.set("Range", &format!("bytes={have}-"));
            }
            match request.call() {
                Ok(r) => break r,
                // The server does not like our range (the file changed): start from zero.
                Err(ureq::Error::Status(416, _)) if have > 0 => {
                    fs::remove_file(part).map_err(|e| io_err(part, e))?;
                    have = 0;
                }
                Err(ureq::Error::Status(code, _)) => return Err(StemError::Http(format!("the server answered {code}"))),
                Err(e) => return Err(StemError::Http(e.to_string())),
            }
        };
        // 206 = the server continues; 200 = it ignored Range and sends the whole file again.
        let append = have > 0 && response.status() == 206;
        let mut file = if append {
            OpenOptions::new().append(true).open(part)
        } else {
            have = 0;
            File::create(part)
        }
        .map_err(|e| io_err(part, e))?;

        let mut reader = response.into_reader();
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = have;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(StemError::Cancelled);
            }
            let n = reader.read(&mut buf).map_err(|e| StemError::Http(e.to_string()))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| io_err(part, e))?;
            done += n as u64;
            progress(Progress::Download { done, total });
        }
        file.flush().map_err(|e| io_err(part, e))
    }

    /// Takes a model file the user already has (any file name): checks size and SHA-256 and copies it into the
    /// models folder. The original is never changed.
    pub fn import(&self, source: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(Progress)) -> Result<(), StemError> {
        let (total, sha) = self.pinned()?;
        let size = fs::metadata(source).map_err(|e| io_err(source, e))?.len();
        if size != total {
            return Err(StemError::Mismatch);
        }
        let digest = hash_file(source, cancel, |done, total| progress(Progress::Verify { done, total }))?;
        if !digest.eq_ignore_ascii_case(sha) {
            return Err(StemError::Mismatch);
        }
        fs::create_dir_all(&self.dir).map_err(|e| io_err(&self.dir, e))?;
        let target = self.model_path();
        if let (Ok(a), Ok(b)) = (fs::canonicalize(source), fs::canonicalize(&target)) {
            if a == b {
                return Ok(());
            }
        }
        let part = self.part_path();
        fs::copy(source, &part).map_err(|e| io_err(&part, e))?;
        replace_file(&part, &target)
    }

    /// Removes the model and any half-loaded download.
    pub fn delete(&self) -> Result<(), StemError> {
        for path in [self.model_path(), self.part_path()] {
            match fs::remove_file(&path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(io_err(&path, e)),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("radiant-stems-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sha256_of_a_file_matches_the_known_vector() {
        let dir = temp_dir("hash");
        let path = dir.join("abc.bin");
        fs::write(&path, b"abc").unwrap();
        let no_cancel = AtomicBool::new(false);
        let hash = hash_file(&path, &no_cancel, |_, _| {}).unwrap();
        assert_eq!(hash, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn hashing_can_be_cancelled() {
        let dir = temp_dir("cancel");
        let path = dir.join("x.bin");
        fs::write(&path, vec![1u8; 4096]).unwrap();
        let cancel = AtomicBool::new(true);
        assert!(matches!(hash_file(&path, &cancel, |_, _| {}), Err(StemError::Cancelled)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn state_and_delete() {
        let dir = temp_dir("state");
        let store = ModelStore::new(&dir);
        assert_eq!(store.state(), ModelState::Missing);
        fs::write(store.model_path(), b"not a model").unwrap();
        fs::write(store.part_path(), b"half").unwrap();
        // Unpinned model: any file with the right name counts as present; pinned: the size must match.
        let expected = if MODEL.bytes.is_some() { ModelState::WrongSize } else { ModelState::Ready };
        assert_eq!(store.state(), expected);
        store.delete().unwrap();
        assert_eq!(store.state(), ModelState::Missing);
        assert!(!store.part_path().exists());
        store.delete().unwrap(); // deleting nothing is fine
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn nothing_is_downloaded_or_imported_without_a_pinned_checksum() {
        if MODEL.is_pinned() {
            return;
        }
        let dir = temp_dir("pin");
        let store = ModelStore::new(&dir);
        let no_cancel = AtomicBool::new(false);
        assert!(matches!(store.download(&no_cancel, &mut |_| {}), Err(StemError::NotPinned)));
        assert!(matches!(store.import(&dir.join("x.onnx"), &no_cancel, &mut |_| {}), Err(StemError::NotPinned)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn model_address_is_https() {
        assert!(MODEL.url.starts_with("https://"));
        if let Some(sha) = MODEL.sha256 {
            assert_eq!(sha.len(), 64);
            assert!(sha.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
        }
    }
}
