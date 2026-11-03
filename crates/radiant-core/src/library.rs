//! Library scan (Android `Library.kt`).
//!
//! DONE: recursive walk, extension filter, title from file name, sidecar lyrics detection,
//!       album grouping.
//! TODO: read tags/duration/bitrate with symphonia (needs a fast path: scan must stay quick),
//!       cache results between launches, manual rescan + optional filesystem watcher.
//! Paths are `PathBuf` everywhere: never assume UTF-8 file names (README section 4).

use crate::models::{Album, AudioInfo, Track};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "ogg", "oga", "opus", "m4a", "aac", "alac", "wav",
];

pub fn is_audio_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Recursively scans `root`. Run it off the UI thread. Result is sorted by path.
pub fn scan(root: &Path) -> Vec<Track> {
    let mut out = Vec::new();
    walk(root, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(dir: &Path, out: &mut Vec<Track>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        // `DirEntry::file_type` does not follow symlinks, so no loops.
        let Ok(ft) = entry.file_type() else { continue };
        let path = entry.path();
        if ft.is_dir() {
            walk(&path, out);
        } else if ft.is_file() && is_audio_file(&path) {
            out.push(track_from_path(&path));
        }
    }
}

fn track_from_path(path: &Path) -> Track {
    let meta = fs::metadata(path).ok();
    let format = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_uppercase();
    Track {
        path: path.to_path_buf(),
        title: path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default(),
        info: AudioInfo {
            format,
            size_bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
            ..Default::default()
        },
        modified: meta.as_ref().and_then(|m| m.modified().ok()),
        lyrics_path: sidecar_lyrics(path),
        ..Default::default()
    }
}

/// `.lrc` wins over `.txt` (synced lyrics are better).
fn sidecar_lyrics(track: &Path) -> Option<PathBuf> {
    ["lrc", "txt"]
        .iter()
        .map(|ext| track.with_extension(ext))
        .find(|p| p.is_file())
}

/// Groups by (album artist or artist, album). Unknown values stay `None`.
pub fn group_albums(tracks: &[Track]) -> Vec<Album> {
    let mut map: BTreeMap<(Option<String>, Option<String>), Vec<PathBuf>> = BTreeMap::new();
    for t in tracks {
        let artist = t.album_artist.clone().or_else(|| t.artist.clone());
        map.entry((artist, t.album.clone()))
            .or_default()
            .push(t.path.clone());
    }
    map.into_iter()
        .map(|((artist, name), tracks)| {
            let key = format!(
                "{}\u{1}{}",
                artist.as_deref().unwrap_or(""),
                name.as_deref().unwrap_or("")
            );
            Album { key, name, artist, tracks }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_filter() {
        assert!(is_audio_file(Path::new("a/b/Song.MP3")));
        assert!(is_audio_file(Path::new("x.flac")));
        assert!(!is_audio_file(Path::new("cover.jpg")));
        assert!(!is_audio_file(Path::new("noext")));
    }

    #[test]
    fn scan_finds_nested_tracks_and_lyrics() {
        let root = std::env::temp_dir().join(format!("radiant_scan_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("a.mp3"), b"x").unwrap();
        fs::write(root.join("a.lrc"), b"[00:01.00]hi").unwrap();
        fs::write(root.join("sub").join("b.flac"), b"x").unwrap();
        fs::write(root.join("notes.txt"), b"x").unwrap();

        let tracks = scan(&root);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].title, "a");
        assert!(tracks[0].lyrics_path.is_some());
        assert!(tracks[1].lyrics_path.is_none());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn albums_group_by_artist_and_album() {
        let mk = |artist: &str, album: &str, name: &str| Track {
            path: PathBuf::from(name),
            artist: Some(artist.into()),
            album: Some(album.into()),
            ..Default::default()
        };
        let tracks = vec![mk("A", "X", "1"), mk("A", "X", "2"), mk("B", "Y", "3")];
        let albums = group_albums(&tracks);
        assert_eq!(albums.len(), 2);
        assert_eq!(albums[0].tracks.len(), 2);
    }
}
