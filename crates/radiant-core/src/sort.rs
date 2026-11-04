//! Track sorting (Android `ListTools.kt`).
//! DONE: tracks by name/artist/album/duration/file date, asc/desc.
//! TODO: `Manual` order needs a stored order list (it is a sort mode of its own);
//!       albums and playlists sorting; proper locale-aware collation (now: lowercase compare).

use crate::models::{SortKey, SortState, Track};
use std::cmp::Ordering;

fn cmp_ci(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

/// Missing values (no tag, no duration) always go last, in both directions.
fn cmp_opt<T: Ord>(a: Option<T>, b: Option<T>, descending: bool) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => if descending { y.cmp(&x) } else { x.cmp(&y) },
    }
}

fn text_key(s: &Option<String>) -> Option<String> {
    s.as_deref().filter(|v| !v.trim().is_empty()).map(|v| v.to_lowercase())
}

pub fn sort_tracks(tracks: &mut [Track], state: SortState) {
    if state.key == SortKey::Manual {
        return;
    }
    let d = state.descending;
    tracks.sort_by(|a, b| {
        let o = match state.key {
            SortKey::Manual => Ordering::Equal,
            SortKey::Name => {
                let o = cmp_ci(&a.title, &b.title);
                if d { o.reverse() } else { o }
            }
            SortKey::Artist => cmp_opt(text_key(&a.artist), text_key(&b.artist), d),
            SortKey::Album => cmp_opt(text_key(&a.album), text_key(&b.album), d),
            SortKey::Duration => cmp_opt(a.duration_ms.filter(|x| *x > 0), b.duration_ms.filter(|x| *x > 0), d),
            SortKey::FileDate => cmp_opt(a.modified, b.modified, d),
        };
        // Equal keys keep a stable, predictable order in both directions.
        o.then_with(|| cmp_ci(&a.title, &b.title)).then_with(|| a.path.cmp(&b.path))
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

    #[test]
    fn missing_values_stay_last_in_both_directions() {
        let mk = |title: &str, artist: Option<&str>| Track {
            title: title.into(),
            artist: artist.map(Into::into),
            ..Default::default()
        };
        let mut v = vec![mk("x", None), mk("b", Some("Beta")), mk("a", Some("alpha"))];
        sort_tracks(&mut v, SortState { key: SortKey::Artist, descending: false });
        assert_eq!(v.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["a", "b", "x"]);
        sort_tracks(&mut v, SortState { key: SortKey::Artist, descending: true });
        assert_eq!(v.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["b", "a", "x"]);
    }

    #[test]
    fn equal_keys_fall_back_to_title_in_both_directions() {
        let mut v = vec![t("b"), t("a"), t("c")];
        sort_tracks(&mut v, SortState { key: SortKey::Artist, descending: true });
        assert_eq!(v.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
    }
}
