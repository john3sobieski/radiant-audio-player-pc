//! One-way download: the app folder in Dropbox -> `<music folder>/Dropbox`. Files that already exist are never
//! touched (no overwrite, no delete); nothing in the cloud is changed.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::api::{RemoteFile, Session};
use crate::{is_configured, CloudError};

/// Stop after this many files in a row failed (no internet, disk full, ...).
const MAX_FAILURES_IN_A_ROW: usize = 3;

/// Besides music: lyrics next to a track and cover pictures, because the library uses them.
const EXTRA_EXTENSIONS: &[&str] = &["lrc", "txt", "jpg", "jpeg", "png"];

#[derive(Debug, Clone)]
pub enum Progress {
    /// Reading the list of files; `found` so far.
    Listing { found: usize },
    /// File `index` of `count` (1-based); the bytes are over the whole run.
    Downloading { index: usize, count: usize, name: String, done_bytes: u64, total_bytes: u64 },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Summary {
    pub downloaded: usize,
    /// Wanted files that were already on disk.
    pub present: usize,
    pub failed: usize,
    pub bytes: u64,
    pub cancelled: bool,
}

/// Music, lyrics and covers are downloaded; everything else in the folder is left in the cloud.
pub fn wanted(remote_path: &str) -> bool {
    let name = remote_path.rsplit('/').next().unwrap_or(remote_path);
    let Some((_, ext)) = name.rsplit_once('.') else { return false };
    let ext = ext.to_ascii_lowercase();
    radiant_core::library::AUDIO_EXTENSIONS.contains(&ext.as_str()) || EXTRA_EXTENSIONS.contains(&ext.as_str())
}

fn clean_component(part: &str) -> String {
    let mut s: String = part
        .chars()
        .map(|c| if c.is_control() || "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect();
    while s.ends_with(' ') || s.ends_with('.') {
        s.pop();
    }
    if s.is_empty() {
        s.push('_');
    }
    let stem = s.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT")) && stem.len() == 4 && stem.as_bytes()[3].is_ascii_digit());
    if reserved {
        s.insert(0, '_');
    }
    s
}

/// The place of a cloud path under `dest`; `None` for paths that try to leave it (`..`) or are empty.
/// Characters Windows does not allow in names are replaced.
pub fn local_path(dest: &Path, remote_path: &str) -> Option<PathBuf> {
    let mut out = dest.to_path_buf();
    let mut any = false;
    for part in remote_path.split('/').filter(|p| !p.is_empty()) {
        if part == "." || part == ".." {
            return None;
        }
        out.push(clean_component(part));
        any = true;
    }
    any.then_some(out)
}

fn part_path(target: &Path) -> PathBuf {
    let mut name: OsString = target.file_name().map(OsString::from).unwrap_or_default();
    name.push(".part");
    target.with_file_name(name)
}

/// Copies the new files from Dropbox into `dest`. Blocks: run it on a worker thread. `cancel` stops it between
/// reads (the half-written file is removed); the result then has `cancelled` set.
pub fn sync(
    refresh_token: &str,
    dest: &Path,
    cancel: &AtomicBool,
    on_progress: &mut dyn FnMut(Progress),
) -> Result<Summary, CloudError> {
    if !is_configured() {
        return Err(CloudError::NotConfigured);
    }
    let mut session = Session::start(refresh_token)?;
    let remote = match session.list_files(cancel, &mut |found| on_progress(Progress::Listing { found })) {
        Ok(list) => list,
        Err(CloudError::Cancelled) => return Ok(Summary { cancelled: true, ..Summary::default() }),
        Err(e) => return Err(e),
    };

    let mut summary = Summary::default();
    let mut todo: Vec<(RemoteFile, PathBuf)> = Vec::new();
    for file in remote.into_iter().filter(|f| wanted(&f.path)) {
        let Some(target) = local_path(dest, &file.path) else { continue };
        if target.exists() {
            summary.present += 1;
        } else {
            todo.push((file, target));
        }
    }
    let total_bytes: u64 = todo.iter().map(|(f, _)| f.size).sum();
    let count = todo.len();
    let (mut done_bytes, mut in_a_row) = (0u64, 0usize);
    let mut last_error = CloudError::Api("unknown".into());

    for (i, (file, target)) in todo.into_iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            summary.cancelled = true;
            break;
        }
        let name = file.path.rsplit('/').next().unwrap_or(&file.path).to_string();
        let base = done_bytes;
        let mut report = |file_done: u64| {
            on_progress(Progress::Downloading {
                index: i + 1,
                count,
                name: name.clone(),
                done_bytes: base + file_done,
                total_bytes,
            })
        };
        report(0);
        let part = part_path(&target);
        let result: Result<bool, CloudError> = (|| {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let _ = fs::remove_file(&part);
            session.download(cancel, &file, &part, &mut report)?;
            if target.exists() {
                // Appeared meanwhile: never overwrite.
                let _ = fs::remove_file(&part);
                return Ok(false);
            }
            fs::rename(&part, &target)?;
            Ok(true)
        })();
        match result {
            Ok(true) => {
                summary.downloaded += 1;
                summary.bytes += file.size;
                done_bytes += file.size;
                in_a_row = 0;
            }
            Ok(false) => summary.present += 1,
            Err(CloudError::Cancelled) => {
                let _ = fs::remove_file(&part);
                summary.cancelled = true;
                break;
            }
            Err(e @ (CloudError::AuthExpired | CloudError::NotSignedIn)) => {
                let _ = fs::remove_file(&part);
                return Err(e);
            }
            Err(e) => {
                let _ = fs::remove_file(&part);
                summary.failed += 1;
                done_bytes += file.size;
                in_a_row += 1;
                last_error = e;
                if in_a_row >= MAX_FAILURES_IN_A_ROW {
                    return Err(last_error);
                }
            }
        }
    }
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_music_lyrics_and_covers_are_wanted() {
        assert!(wanted("Artist/Album/01 Song.FLAC"));
        assert!(wanted("a.mp3"));
        assert!(wanted("Artist/Album/01 Song.lrc"));
        assert!(wanted("Artist/Album/cover.jpg"));
        assert!(!wanted("Artist/notes.docx"));
        assert!(!wanted("Artist/noextension"));
    }

    #[test]
    fn local_paths_stay_inside() {
        let dest = Path::new("music").join("Dropbox");
        assert_eq!(local_path(&dest, "A/B/c.mp3"), Some(dest.join("A").join("B").join("c.mp3")));
        assert_eq!(local_path(&dest, "/A/c.mp3"), Some(dest.join("A").join("c.mp3")));
        assert_eq!(local_path(&dest, "../x.mp3"), None);
        assert_eq!(local_path(&dest, "A/../../x.mp3"), None);
        assert_eq!(local_path(&dest, ""), None);
    }

    #[test]
    fn names_are_made_safe_for_windows() {
        assert_eq!(clean_component("a:b?c"), "a_b_c");
        assert_eq!(clean_component("song. "), "song");
        assert_eq!(clean_component("..."), "_");
        assert_eq!(clean_component("CON.mp3"), "_CON.mp3");
        assert_eq!(clean_component("com1"), "_com1");
        assert_eq!(clean_component("Привет — 日本.mp3"), "Привет — 日本.mp3");
    }

    #[test]
    fn part_file_sits_next_to_the_target() {
        assert_eq!(part_path(Path::new("x/y.mp3")), Path::new("x/y.mp3.part"));
    }
}
