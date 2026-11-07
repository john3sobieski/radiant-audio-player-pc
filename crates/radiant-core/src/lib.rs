//! radiant-core: data model, library scan, sorting. No audio, no UI.
//!
//! DONE: basic models, recursive folder scan (file names only), album grouping, track sorting,
//!       JSON settings (music folder, volume, shuffle, repeat, speed, sound),
//!       playlists + playlist folders (`playlists.json`), the four queues (`queues.json`).
//! TODO: app-lock logic. See PROGRESS.md.

pub mod library;
pub mod lyrics;
pub mod models;
pub mod playlists;
pub mod queues;
pub mod settings;
pub mod sort;
