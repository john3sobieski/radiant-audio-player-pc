//! radiant-core: data model, library scan, sorting. No audio, no UI.
//!
//! DONE: basic models, recursive folder scan (file names only), album grouping, track sorting,
//!       JSON settings (music folder, volume, shuffle, repeat, speed, sound),
//!       playlists + playlist folders (`playlists.json`), the four queues (`queues.json`).
//!       app-lock logic: code hashing, failure delays, `applock.json`, 12-word recovery phrase (`wordlist.rs`).
//! TODO: see PROGRESS.md.

pub mod applock;
pub mod id3;
pub mod library;
pub mod lyrics;
pub mod models;
pub mod playlists;
pub mod queues;
pub mod settings;
pub mod sort;
pub mod theme;
pub mod wordlist;
