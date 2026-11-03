//! radiant-core: data model, library scan, sorting. No audio, no UI.
//!
//! DONE: basic models, recursive folder scan (file names only), album grouping, track sorting.
//! TODO: read real tags/duration/bitrate (symphonia), playlists + queues logic, search,
//!       settings persistence, lyrics parsing, app-lock logic. See PROGRESS.md.

pub mod library;
pub mod models;
pub mod sort;
