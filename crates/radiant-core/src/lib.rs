//! radiant-core: data model, library scan, sorting. No audio, no UI.
//!
//! DONE: basic models, recursive folder scan (file names only), album grouping, track sorting,
//!       JSON settings (music folder, volume, shuffle, repeat, speed, sound).
//! TODO: read real tags/duration/bitrate (symphonia), playlists + queues logic, search,
//!       lyrics parsing, app-lock logic. See PROGRESS.md.

pub mod library;
pub mod models;
pub mod settings;
pub mod sort;
