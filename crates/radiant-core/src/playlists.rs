//! Playlists and playlist folders (README 3.1 / 3.2): data, rules and the `playlists.json` file.
//! No UI here. "Liked tracks" (`FAVORITES_ID`) always exists, is pinned on top, has no folder.

use crate::models::{Playlist, PlaylistFolder, FAVORITES_ID};
use crate::settings::{replace_file, settings_path};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const PLAYLISTS_FILE: &str = "playlists.json";
/// Longest playlist or folder name, in characters.
pub const NAME_MAX_CHARS: usize = 60;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaylistStore {
    pub folders: Vec<PlaylistFolder>,
    pub playlists: Vec<Playlist>,
}

/// One line of the Playlists list (indexes into the store).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListEntry {
    Folder(usize),
    Playlist(usize),
}

/// Trims, joins runs of whitespace, cuts to `NAME_MAX_CHARS`. `None` = nothing left.
pub fn clean_name(raw: &str) -> Option<String> {
    let joined = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if joined.is_empty() {
        None
    } else {
        Some(joined.chars().take(NAME_MAX_CHARS).collect())
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn make_id(prefix: &str, taken: impl Fn(&str) -> bool) -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    let mut n = 0_u32;
    loop {
        let id = if n == 0 { format!("{prefix}{nanos:x}") } else { format!("{prefix}{nanos:x}-{n}") };
        if !taken(&id) {
            return id;
        }
        n += 1;
    }
}

impl PlaylistStore {
    /// Repairs a hand-edited or older file: Liked tracks first and folder-less, unique ids,
    /// no folder ids that point nowhere, no track twice in one playlist.
    pub fn normalize(&mut self) {
        let mut seen = HashSet::new();
        self.folders.retain(|f| seen.insert(f.id.clone()));
        let folder_ids: HashSet<String> = self.folders.iter().map(|f| f.id.clone()).collect();
        let mut seen = HashSet::new();
        self.playlists.retain(|p| seen.insert(p.id.clone()));
        for p in &mut self.playlists {
            if p.folder_id.as_ref().map_or(false, |f| !folder_ids.contains(f)) {
                p.folder_id = None;
            }
            let mut tracks = HashSet::new();
            p.track_paths.retain(|t| tracks.insert(t.clone()));
        }
        let mut liked = match self.playlists.iter().position(|p| p.id == FAVORITES_ID) {
            Some(i) => self.playlists.remove(i),
            None => Playlist { id: FAVORITES_ID.to_string(), name: "Liked tracks".to_string(), ..Default::default() },
        };
        liked.folder_id = None;
        self.playlists.insert(0, liked);
    }

    pub fn playlist(&self, id: &str) -> Option<&Playlist> {
        self.playlists.iter().find(|p| p.id == id)
    }

    pub fn folder(&self, id: &str) -> Option<&PlaylistFolder> {
        self.folders.iter().find(|f| f.id == id)
    }

    pub fn contains(&self, id: &str, track: &Path) -> bool {
        self.playlist(id).map_or(false, |p| p.track_paths.iter().any(|t| t == track))
    }

    /// New folder; `None` when the name is empty.
    pub fn create_folder(&mut self, name: &str) -> Option<String> {
        let name = clean_name(name)?;
        let id = make_id("f", |id| self.folders.iter().any(|f| f.id == id));
        self.folders.push(PlaylistFolder { id: id.clone(), name, collapsed: false });
        Some(id)
    }

    /// New playlist (optionally inside a folder); `None` when the name is empty.
    pub fn create_playlist(&mut self, name: &str, folder_id: Option<&str>) -> Option<String> {
        let name = clean_name(name)?;
        let id = make_id("p", |id| self.playlists.iter().any(|p| p.id == id));
        let folder_id = folder_id.filter(|f| self.folder(f).is_some()).map(str::to_string);
        let now = now_secs();
        self.playlists.push(Playlist { id: id.clone(), name, track_paths: Vec::new(), created_at: now, updated_at: now, folder_id });
        Some(id)
    }

    /// Adds a track to the end. `false` = unknown playlist or the track is already there.
    pub fn add_track(&mut self, id: &str, track: &Path) -> bool {
        let Some(p) = self.playlists.iter_mut().find(|p| p.id == id) else { return false };
        if p.track_paths.iter().any(|t| t == track) {
            return false;
        }
        p.track_paths.push(track.to_path_buf());
        p.updated_at = now_secs();
        true
    }

    pub fn remove_track(&mut self, id: &str, track: &Path) -> bool {
        let Some(p) = self.playlists.iter_mut().find(|p| p.id == id) else { return false };
        let before = p.track_paths.len();
        p.track_paths.retain(|t| t != track);
        let changed = p.track_paths.len() != before;
        if changed {
            p.updated_at = now_secs();
        }
        changed
    }

    /// Moves the track at `from` to position `to` (manual order). `false` = nothing changed.
    pub fn move_track(&mut self, id: &str, from: usize, to: usize) -> bool {
        let Some(p) = self.playlists.iter_mut().find(|p| p.id == id) else { return false };
        if from == to || from >= p.track_paths.len() || to >= p.track_paths.len() {
            return false;
        }
        let track = p.track_paths.remove(from);
        p.track_paths.insert(to, track);
        true
    }

    /// Moves a playlist into a folder (`None` = out of any folder). Liked tracks cannot be moved.
    pub fn set_playlist_folder(&mut self, id: &str, folder_id: Option<&str>) -> bool {
        if id == FAVORITES_ID || folder_id.map_or(false, |f| self.folder(f).is_none()) {
            return false;
        }
        let Some(p) = self.playlists.iter_mut().find(|p| p.id == id) else { return false };
        p.folder_id = folder_id.map(str::to_string);
        true
    }

    /// Renames a playlist. `false` = empty name, unknown playlist, or Liked tracks (cannot be renamed).
    pub fn rename_playlist(&mut self, id: &str, name: &str) -> bool {
        let Some(name) = clean_name(name) else { return false };
        if id == FAVORITES_ID {
            return false;
        }
        match self.playlists.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                p.name = name;
                p.updated_at = now_secs();
                true
            }
            None => false,
        }
    }

    /// Deletes a playlist (the tracks themselves are untouched). Liked tracks cannot be deleted.
    pub fn delete_playlist(&mut self, id: &str) -> bool {
        if id == FAVORITES_ID {
            return false;
        }
        let before = self.playlists.len();
        self.playlists.retain(|p| p.id != id);
        self.playlists.len() != before
    }

    pub fn rename_folder(&mut self, id: &str, name: &str) -> bool {
        let Some(name) = clean_name(name) else { return false };
        match self.folders.iter_mut().find(|f| f.id == id) {
            Some(f) => {
                f.name = name;
                true
            }
            None => false,
        }
    }

    /// Deletes a folder; its playlists are kept and become folder-less.
    pub fn delete_folder(&mut self, id: &str) -> bool {
        let before = self.folders.len();
        self.folders.retain(|f| f.id != id);
        if self.folders.len() == before {
            return false;
        }
        for p in &mut self.playlists {
            if p.folder_id.as_deref() == Some(id) {
                p.folder_id = None;
            }
        }
        true
    }

    pub fn toggle_folder_collapsed(&mut self, id: &str) -> bool {
        match self.folders.iter_mut().find(|f| f.id == id) {
            Some(f) => {
                f.collapsed = !f.collapsed;
                true
            }
            None => false,
        }
    }

    /// Order of the Playlists page: Liked tracks, playlists without a folder (oldest first),
    /// then every folder (by name) followed by its playlists unless it is folded.
    pub fn list_entries(&self) -> Vec<ListEntry> {
        let mut out = Vec::new();
        for (i, p) in self.playlists.iter().enumerate() {
            if p.id == FAVORITES_ID {
                out.push(ListEntry::Playlist(i));
            }
        }
        for (i, p) in self.playlists.iter().enumerate() {
            if p.id != FAVORITES_ID && p.folder_id.is_none() {
                out.push(ListEntry::Playlist(i));
            }
        }
        for fi in self.folder_order() {
            out.push(ListEntry::Folder(fi));
            let folder = &self.folders[fi];
            if folder.collapsed {
                continue;
            }
            for (i, p) in self.playlists.iter().enumerate() {
                if p.folder_id.as_deref() == Some(folder.id.as_str()) {
                    out.push(ListEntry::Playlist(i));
                }
            }
        }
        out
    }

    /// Folder indexes by name.
    pub fn folder_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.folders.len()).collect();
        order.sort_by_key(|&i| self.folders[i].name.to_lowercase());
        order
    }

    /// Order of the "Add to playlist" window: Liked tracks, then the most recently changed first.
    pub fn popup_order(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.playlists.len()).filter(|&i| self.playlists[i].id != FAVORITES_ID).collect();
        order.sort_by_key(|&i| std::cmp::Reverse((self.playlists[i].updated_at, i)));
        if let Some(i) = self.playlists.iter().position(|p| p.id == FAVORITES_ID) {
            order.insert(0, i);
        }
        order
    }
}

pub fn playlists_path() -> PathBuf {
    settings_path().with_file_name(PLAYLISTS_FILE)
}

pub fn load() -> Result<PlaylistStore, String> {
    read_at(&playlists_path())
}

pub fn save(store: &PlaylistStore) -> Result<(), String> {
    write_at(&playlists_path(), store)
}

fn read_at(path: &Path) -> Result<PlaylistStore, String> {
    match fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str::<PlaylistStore>(&text) {
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
            let mut store = PlaylistStore::default();
            store.normalize();
            Ok(store)
        }
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write_at(path: &Path, store: &PlaylistStore) -> Result<(), String> {
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

    fn fresh() -> PlaylistStore {
        let mut s = PlaylistStore::default();
        s.normalize();
        s
    }

    #[test]
    fn names_are_cleaned() {
        assert_eq!(clean_name("  My   mix \n"), Some("My mix".to_string()));
        assert_eq!(clean_name("   "), None);
        assert_eq!(clean_name(&"я".repeat(100)).unwrap().chars().count(), NAME_MAX_CHARS);
    }

    #[test]
    fn liked_tracks_always_first() {
        let mut s = fresh();
        assert_eq!(s.playlists[0].id, FAVORITES_ID);
        s.playlists.push(Playlist { id: "x".into(), name: "X".into(), ..Default::default() });
        let mut liked = s.playlists.remove(0);
        liked.folder_id = Some("gone".into());
        s.playlists.push(liked);
        s.normalize();
        assert_eq!(s.playlists[0].id, FAVORITES_ID);
        assert_eq!(s.playlists[0].folder_id, None);
        assert_eq!(s.playlists.len(), 2);
    }

    #[test]
    fn create_add_remove() {
        let mut s = fresh();
        let id = s.create_playlist(" Road ", None).unwrap();
        assert_eq!(s.playlist(&id).unwrap().name, "Road");
        assert!(s.create_playlist("  ", None).is_none());
        let t = Path::new("a.mp3");
        assert!(s.add_track(&id, t));
        assert!(!s.add_track(&id, t));
        assert!(s.contains(&id, t));
        assert!(s.remove_track(&id, t));
        assert!(!s.remove_track(&id, t));
        assert!(!s.add_track("nope", t));
    }

    #[test]
    fn folders_and_moving() {
        let mut s = fresh();
        let f = s.create_folder("Gym").unwrap();
        let p = s.create_playlist("Run", Some(&f)).unwrap();
        assert_eq!(s.playlist(&p).unwrap().folder_id.as_deref(), Some(f.as_str()));
        // A folder that does not exist is ignored.
        let q = s.create_playlist("Walk", Some("missing")).unwrap();
        assert_eq!(s.playlist(&q).unwrap().folder_id, None);
        assert!(s.set_playlist_folder(&q, Some(&f)));
        assert!(!s.set_playlist_folder(&q, Some("missing")));
        assert!(!s.set_playlist_folder(FAVORITES_ID, Some(&f)));
        assert!(s.set_playlist_folder(&q, None));
        assert_eq!(s.playlist(&q).unwrap().folder_id, None);
    }

    #[test]
    fn rename_and_delete() {
        let mut s = fresh();
        let f = s.create_folder("Gym").unwrap();
        let p = s.create_playlist("Run", Some(&f)).unwrap();
        assert!(s.rename_playlist(&p, "  Fast   run "));
        assert_eq!(s.playlist(&p).unwrap().name, "Fast run");
        assert!(!s.rename_playlist(&p, "   "));
        assert!(!s.rename_playlist(FAVORITES_ID, "Mine"));
        assert!(!s.rename_playlist("nope", "x"));
        assert!(s.rename_folder(&f, "Sport"));
        assert_eq!(s.folder(&f).unwrap().name, "Sport");
        assert!(!s.rename_folder(&f, ""));

        // Deleting a folder keeps its playlists.
        assert!(s.delete_folder(&f));
        assert_eq!(s.playlist(&p).unwrap().folder_id, None);
        assert!(!s.delete_folder(&f));

        assert!(!s.delete_playlist(FAVORITES_ID));
        assert!(s.delete_playlist(&p));
        assert!(s.playlist(&p).is_none());
        assert!(!s.delete_playlist(&p));
        assert_eq!(s.playlists[0].id, FAVORITES_ID);
    }

    #[test]
    fn manual_track_order() {
        let mut s = fresh();
        let id = s.create_playlist("Mix", None).unwrap();
        for name in ["a.mp3", "b.mp3", "c.mp3", "d.mp3"] {
            s.add_track(&id, Path::new(name));
        }
        let names = |s: &PlaylistStore| -> Vec<String> {
            s.playlist(&id).unwrap().track_paths.iter().map(|p| p.to_string_lossy().into_owned()).collect()
        };
        assert!(s.move_track(&id, 0, 2));
        assert_eq!(names(&s), ["b.mp3", "c.mp3", "a.mp3", "d.mp3"]);
        assert!(s.move_track(&id, 3, 0));
        assert_eq!(names(&s), ["d.mp3", "b.mp3", "c.mp3", "a.mp3"]);
        assert!(!s.move_track(&id, 1, 1));
        assert!(!s.move_track(&id, 1, 4));
        assert!(!s.move_track(&id, 9, 0));
        assert!(!s.move_track("nope", 0, 1));
    }

    #[test]
    fn list_order_and_folding() {
        let mut s = fresh();
        let b = s.create_folder("beta").unwrap();
        let a = s.create_folder("Alpha").unwrap();
        let in_b = s.create_playlist("in b", Some(&b)).unwrap();
        let loose = s.create_playlist("loose", None).unwrap();
        let in_a = s.create_playlist("in a", Some(&a)).unwrap();
        let idx = |s: &PlaylistStore, id: &str| s.playlists.iter().position(|p| p.id == id).unwrap();
        let fa = s.folders.iter().position(|f| f.id == a).unwrap();
        let fb = s.folders.iter().position(|f| f.id == b).unwrap();
        let expect = vec![
            ListEntry::Playlist(0),
            ListEntry::Playlist(idx(&s, &loose)),
            ListEntry::Folder(fa),
            ListEntry::Playlist(idx(&s, &in_a)),
            ListEntry::Folder(fb),
            ListEntry::Playlist(idx(&s, &in_b)),
        ];
        assert_eq!(s.list_entries(), expect);
        assert!(s.toggle_folder_collapsed(&a));
        assert_eq!(s.list_entries().len(), expect.len() - 1);
    }

    #[test]
    fn popup_order_liked_then_newest() {
        let mut s = fresh();
        let old = s.create_playlist("old", None).unwrap();
        let new = s.create_playlist("new", None).unwrap();
        s.playlists.iter_mut().find(|p| p.id == old).unwrap().updated_at = 10;
        s.playlists.iter_mut().find(|p| p.id == new).unwrap().updated_at = 20;
        let ids: Vec<&str> = s.popup_order().into_iter().map(|i| s.playlists[i].id.as_str()).collect();
        assert_eq!(ids, vec![FAVORITES_ID, new.as_str(), old.as_str()]);
    }

    #[test]
    fn file_round_trip_and_broken_file() {
        let dir = env::temp_dir().join(format!("radiant-playlists-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("playlists.json");

        let mut s = fresh();
        let f = s.create_folder("Музыка").unwrap();
        let p = s.create_playlist("Дорога", Some(&f)).unwrap();
        s.add_track(&p, Path::new("D:/Музыка/а.mp3"));
        write_at(&path, &s).unwrap();
        assert_eq!(read_at(&path).unwrap(), s);
        assert!(!path.with_extension("json.tmp").exists());

        fs::write(&path, "{not json").unwrap();
        assert!(read_at(&path).is_err());
        assert_eq!(fs::read_to_string(path.with_extension("json.bad")).unwrap(), "{not json");
        let _ = fs::remove_dir_all(&dir);
    }
}
