//! Data model, ported from Android `Models.kt` (README section 3.2).
//! Tracks are identified by their file path (Android used a URI).
//! `None` for album/artist means "unknown": the UI shows a translated placeholder,
//! the translated text is never stored.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::SystemTime;

pub const FAVORITES_ID: &str = "favorites";
/// The folder every playlist file has: finished stems are added to playlists inside it. It cannot be renamed or
/// deleted; its shown name is translated by the interface (`PlaylistFolder::name` keeps the English "Stems").
pub const STEMS_FOLDER_ID: &str = "stems";
pub const QUEUE_COUNT: usize = 4;

#[derive(Debug, Clone, Default)]
pub struct AudioInfo {
    /// File extension in capitals ("FLAC", "M4A", ...).
    pub format: String,
    /// Codec found in the stream ("FLAC", "ALAC", "AAC", "MP3", "Vorbis", "Opus", "WAV"); empty = unknown.
    /// Tells apart what the extension cannot (an .m4a can hold AAC or ALAC).
    pub codec: String,
    /// Bits per sample of the stream (lossless formats), if known.
    pub bit_depth: Option<u32>,
    /// The codec keeps every sample as it was (FLAC, ALAC, WAV, ...).
    pub lossless: bool,
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
    /// Disc number from the tag ("2" or "2/3"), if present.
    pub disc_no: Option<u32>,
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

/// A playlist. Saved in `playlists.json` (see `playlists.rs`); times are Unix seconds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub track_paths: Vec<PathBuf>,
    pub created_at: u64,
    pub updated_at: u64,
    /// Folder the playlist lives in; `None` = no folder.
    pub folder_id: Option<String>,
}

/// A folder that groups playlists (one level only, no folders inside folders).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaylistFolder {
    pub id: String,
    pub name: String,
    /// Folded in the Playlists list.
    pub collapsed: bool,
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
