//! Track sorting (Android `ListTools.kt`).
//! DONE: tracks by name/artist/album/duration/file date, asc/desc.
//! TODO: `Manual` order needs a stored order list (it is a sort mode of its own);
//!       albums and playlists sorting; proper locale-aware collation (now: lowercase compare).

use crate::models::{SortKey, SortState, Track};
use std::cmp::Ordering;

fn cmp_ci(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

fn opt(s: &Option<String>) -> &str {
    s.as_deref().unwrap_or("")
}

pub fn sort_tracks(tracks: &mut [Track], state: SortState) {
    if state.key == SortKey::Manual {
        return;
    }
    tracks.sort_by(|a, b| {
        let o = match state.key {
            SortKey::Manual => Ordering::Equal,
            SortKey::Name => cmp_ci(&a.title, &b.title),
            SortKey::Artist => cmp_ci(opt(&a.artist), opt(&b.artist)),
            SortKey::Album => cmp_ci(opt(&a.album), opt(&b.album)),
            SortKey::Duration => a.duration_ms.cmp(&b.duration_ms),
            SortKey::FileDate => a.modified.cmp(&b.modified),
        };
        if state.descending { o.reverse() } else { o }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(title: &str) -> Track {
        Track { title: title.into(), ..Default::default() }
    }

    #[test]
    fn sorts_by_name_both_directions() {
        let mut v = vec![t("b"), t("A"), t("c")];
        sort_tracks(&mut v, SortState { key: SortKey::Name, descending: false });
        assert_eq!(v.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), ["A", "b", "c"]);
        sort_tracks(&mut v, SortState { key: SortKey::Name, descending: true });
        assert_eq!(v[0].title, "c");
    }
}
