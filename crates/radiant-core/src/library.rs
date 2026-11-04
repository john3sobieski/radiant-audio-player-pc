//! Library scan (Android `Library.kt`).
//!
//! DONE: recursive walk, extension filter, metadata tags and stream details via Symphonia,
//!       sidecar lyrics detection, album grouping.
//! TODO: cache results between launches, manual rescan + optional filesystem watcher.
//! Paths are `PathBuf` everywhere: never assume UTF-8 file names (README section 4).

use crate::models::{Album, AudioInfo, Track};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use symphonia::core::codecs::CODEC_TYPE_NULL;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, StandardTagKey, Value};
use symphonia::core::probe::Hint;

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
    let mut track = Track {
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
    };
    read_audio_metadata(path, &mut track);
    track.info.bitrate_kbps = track.duration_ms.filter(|duration| *duration > 0).map(|duration| {
        ((track.info.size_bytes as u128 * 8) / duration as u128).min(u32::MAX as u128) as u32
    });
    track
}

/// Reads container tags and technical details without decoding audio. Invalid or unsupported files
/// remain in the library with their filename as the title.
fn read_audio_metadata(path: &Path, track: &mut Track) {
    let Ok(file) = fs::File::open(path) else { return };
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let source = MediaSourceStream::new(Box::new(file), Default::default());
    let Ok(probed) = symphonia::default::get_probe().format(
        &hint,
        source,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) else {
        return;
    };
    let mut format = probed.format;
    let Some(audio_track) = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
    else {
        return;
    };
    let params = &audio_track.codec_params;
    track.info.sample_rate = params.sample_rate;
    track.info.channels = params.channels.map(|c| c.count().min(u8::MAX as usize) as u8);
    track.duration_ms = params
        .n_frames
        .zip(params.sample_rate)
        .and_then(|(frames, rate)| {
            (rate > 0).then(|| (frames as u128 * 1000 / rate as u128).min(u64::MAX as u128) as u64)
        });

    let Some(revision) = format.metadata().current() else {
        return;
    };
    let mut tags = HashMap::new();
    for tag in revision.tags() {
        let Some(key) = tag.std_key else { continue };
        let value = match &tag.value {
            Value::String(value) => value.trim().to_owned(),
            Value::UnsignedInt(value) => value.to_string(),
            Value::SignedInt(value) => value.to_string(),
            Value::Float(value) => value.to_string(),
            Value::Boolean(value) => value.to_string(),
            Value::Flag => String::new(),
            Value::Binary(_) => continue,
            _ => continue,
        };
        if !value.is_empty() {
            tags.entry(key).or_insert(value);
        }
    }
    if let Some(title) = tags.get(&StandardTagKey::TrackTitle) {
        track.title.clone_from(title);
    }
    track.artist = tags.get(&StandardTagKey::Artist).cloned();
    track.album_artist = tags.get(&StandardTagKey::AlbumArtist).cloned();
    track.album = tags.get(&StandardTagKey::Album).cloned();
    track.track_no = tags
        .get(&StandardTagKey::TrackNumber)
        .and_then(|v| v.split('/').next()?.trim().parse().ok());
    track.tag_lyrics = tags.contains_key(&StandardTagKey::Lyrics);
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
