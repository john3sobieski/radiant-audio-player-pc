//! Data model, ported from Android `Models.kt` (README section 3.2).
//! Tracks are identified by their file path (Android used a URI).
//! `None` for album/artist means "unknown": the UI shows a translated placeholder,
//! the translated text is never stored.

use std::path::PathBuf;
use std::time::SystemTime;

pub const FAVORITES_ID: &str = "favorites";
pub const QUEUE_COUNT: usize = 4;

#[derive(Debug, Clone, Default)]
pub struct AudioInfo {
    pub format: String,
    pub bitrate_kbps: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    pub track_no: Option<u32>,
    /// Release year from the date tag (4 digits), if present.
    pub year: Option<u32>,
    pub info: AudioInfo,
    pub modified: Option<SystemTime>,
    pub added: Option<SystemTime>,
    /// Sidecar `.lrc` / `.txt` next to the track, if any.
    pub lyrics_path: Option<PathBuf>,
    /// True when lyrics are embedded in tags.
    pub tag_lyrics: bool,
}

#[derive(Debug, Clone)]
pub struct Album {
    pub key: String,
    pub name: Option<String>,
    pub artist: Option<String>,
    pub tracks: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub track_paths: Vec<PathBuf>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// One of exactly four fixed queues (id 0..QUEUE_COUNT).
#[derive(Debug, Clone, Default)]
pub struct PlayQueue {
    pub id: u8,
    pub track_paths: Vec<PathBuf>,
}

/// Track sort modes (README 3.1). TODO: separate key enums for albums and playlists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Manual,
    Name,
    Artist,
    Album,
    Duration,
    FileDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SortState {
    pub key: SortKey,
    pub descending: bool,
}
