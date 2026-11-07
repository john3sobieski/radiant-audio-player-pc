//! The four fixed queues (README 3.1): which tracks each one holds, and the `queues.json` file.
//! No UI here. A track is in a queue at most once; the order is the order of adding.

use crate::models::QUEUE_COUNT;
use crate::settings::{replace_file, settings_path};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const QUEUES_FILE: &str = "queues.json";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QueueStore {
    /// Always `QUEUE_COUNT` lists after `normalize`.
    pub queues: Vec<Vec<PathBuf>>,
}

impl QueueStore {
    /// Repairs a hand-edited file: exactly `QUEUE_COUNT` queues, no track twice in one queue.
    pub fn normalize(&mut self) {
        self.queues.resize_with(QUEUE_COUNT, Vec::new);
        for q in &mut self.queues {
            let mut seen = HashSet::new();
            q.retain(|t| seen.insert(t.clone()));
        }
    }

    pub fn tracks(&self, queue: usize) -> &[PathBuf] {
        self.queues.get(queue).map_or(&[], |q| q.as_slice())
    }

    pub fn contains(&self, queue: usize, track: &Path) -> bool {
        self.tracks(queue).iter().any(|t| t == track)
    }

    /// Adds a track to the end. `false` = unknown queue or the track is already there.
    pub fn add(&mut self, queue: usize, track: &Path) -> bool {
        if self.contains(queue, track) {
            return false;
        }
        match self.queues.get_mut(queue) {
            Some(q) => {
                q.push(track.to_path_buf());
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, queue: usize, track: &Path) -> bool {
        let Some(q) = self.queues.get_mut(queue) else { return false };
        let before = q.len();
        q.retain(|t| t != track);
        q.len() != before
    }

    /// Moves the track at `from` to position `to`. `false` = nothing changed.
    pub fn move_track(&mut self, queue: usize, from: usize, to: usize) -> bool {
        let Some(q) = self.queues.get_mut(queue) else { return false };
        if from == to || from >= q.len() || to >= q.len() {
            return false;
        }
        let track = q.remove(from);
        q.insert(to, track);
        true
    }

    /// Empties one queue. `false` = it was already empty (or unknown).
    pub fn clear(&mut self, queue: usize) -> bool {
        match self.queues.get_mut(queue) {
            Some(q) if !q.is_empty() => {
                q.clear();
                true
            }
            _ => false,
        }
    }

    /// Track count of every queue.
    pub fn counts(&self) -> Vec<usize> {
        (0..QUEUE_COUNT).map(|i| self.tracks(i).len()).collect()
    }
}

pub fn queues_path() -> PathBuf {
    settings_path().with_file_name(QUEUES_FILE)
}

pub fn load() -> Result<QueueStore, String> {
    read_at(&queues_path())
}

pub fn save(store: &QueueStore) -> Result<(), String> {
    write_at(&queues_path(), store)
}

fn read_at(path: &Path) -> Result<QueueStore, String> {
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<QueueStore>(&text) {
            Ok(mut store) => {
                store.normalize();
                Ok(store)
            }
            Err(e) => {
                // Keep a copy: the next save would overwrite the broken file.
                let _ = fs::copy(path, path.with_extension("json.bad"));
                Err(format!("{}: {e}", path.display()))
            }
        },
        Err(e) if e.kind() == ErrorKind::NotFound => {
            let mut store = QueueStore::default();
            store.normalize();
            Ok(store)
        }
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write_at(path: &Path, store: &QueueStore) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
    }
    let mut data = serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?;
    data.push(b'\n');
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    replace_file(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn fresh() -> QueueStore {
        let mut s = QueueStore::default();
        s.normalize();
        s
    }

    #[test]
    fn always_four_queues() {
        assert_eq!(fresh().queues.len(), QUEUE_COUNT);
        let mut s = QueueStore { queues: vec![vec![PathBuf::from("a.mp3"), PathBuf::from("a.mp3")]; 9] };
        s.normalize();
        assert_eq!(s.queues.len(), QUEUE_COUNT);
        assert_eq!(s.tracks(0).len(), 1);
        assert!(s.tracks(9).is_empty());
    }

    #[test]
    fn add_remove_counts() {
        let mut s = fresh();
        let t = Path::new("a.mp3");
        assert!(s.add(1, t));
        assert!(!s.add(1, t));
        assert!(s.add(3, t));
        assert!(!s.add(4, t));
        assert!(s.contains(1, t));
        assert!(!s.contains(0, t));
        assert_eq!(s.counts(), vec![0, 1, 0, 1]);
        assert!(s.remove(1, t));
        assert!(!s.remove(1, t));
        assert!(!s.remove(7, t));
        assert_eq!(s.counts(), vec![0, 0, 0, 1]);
    }

    #[test]
    fn move_and_clear() {
        let mut s = fresh();
        for n in ["a.mp3", "b.mp3", "c.mp3"] {
            s.add(0, Path::new(n));
        }
        assert!(s.move_track(0, 0, 2));
        assert_eq!(s.tracks(0), [PathBuf::from("b.mp3"), PathBuf::from("c.mp3"), PathBuf::from("a.mp3")]);
        assert!(!s.move_track(0, 1, 1));
        assert!(!s.move_track(0, 0, 3));
        assert!(!s.move_track(5, 0, 1));
        assert!(s.clear(0));
        assert!(!s.clear(0));
        assert!(s.tracks(0).is_empty());
    }

    #[test]
    fn file_round_trip_and_broken_file() {
        let dir = env::temp_dir().join(format!("radiant-queues-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("queues.json");

        let mut s = fresh();
        s.add(2, Path::new("D:/Музыка/а.mp3"));
        write_at(&path, &s).unwrap();
        assert_eq!(read_at(&path).unwrap(), s);
        assert!(!path.with_extension("json.tmp").exists());

        fs::write(&path, "{not json").unwrap();
        assert!(read_at(&path).is_err());
        assert_eq!(fs::read_to_string(path.with_extension("json.bad")).unwrap(), "{not json");
        let _ = fs::remove_dir_all(&dir);
    }
}
