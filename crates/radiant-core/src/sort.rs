//! Track sorting (Android `ListTools.kt`).
//! DONE: tracks by name/artist/album/duration/file date, asc/desc.
//! TODO: `Manual` order needs a stored order list (it is a sort mode of its own);
//!       albums and playlists sorting; proper locale-aware collation (now: lowercase compare).

use crate::models::{SortKey, SortState, Track};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::PathBuf;

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
            // Artist -> album -> track number; Album -> artist -> track number (both ascending inside a group).
            SortKey::Artist => cmp_opt(text_key(&a.artist), text_key(&b.artist), d)
                .then_with(|| cmp_opt(text_key(&a.album), text_key(&b.album), false))
                .then_with(|| cmp_opt(a.track_no, b.track_no, false)),
            SortKey::Album => cmp_opt(text_key(&a.album), text_key(&b.album), d)
                .then_with(|| cmp_opt(text_key(&a.artist), text_key(&b.artist), false))
                .then_with(|| cmp_opt(a.track_no, b.track_no, false)),
            SortKey::Duration => cmp_opt(a.duration_ms.filter(|x| *x > 0), b.duration_ms.filter(|x| *x > 0), d),
            SortKey::FileDate => cmp_opt(a.modified, b.modified, d),
        };
        // Equal keys keep a stable, predictable order in both directions.
        o.then_with(|| cmp_ci(&a.title, &b.title)).then_with(|| a.path.cmp(&b.path))
    });
}

/// Score of one text field against the lowercase search text: whole match > start of the field >
/// start of a word > anywhere. A match that covers more of the field scores a bit higher.
fn field_score(field: &str, needle: &str) -> Option<u32> {
    let f = field.to_lowercase();
    if f.is_empty() {
        return None;
    }
    let base = if f == needle {
        100
    } else if f.starts_with(needle) {
        80
    } else if f.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(needle)) {
        60
    } else if f.contains(needle) {
        40
    } else {
        return None;
    };
    let needle_len = needle.chars().count() as u32;
    let field_len = f.chars().count().max(1) as u32;
    Some(base + (needle_len * 20 / field_len).min(20))
}

/// Search relevance of a track (`None` = does not match). `needle` must be trimmed and lowercase.
/// Title counts more than artist and album. When the text has several words, a track where
/// every word appears somewhere (title, artist or album) still matches, with a low score.
pub fn search_score(track: &Track, needle: &str) -> Option<u32> {
    if needle.is_empty() {
        return Some(0);
    }
    let by_field = [
        field_score(&track.title, needle).map(|s| s * 10),
        track.artist.as_deref().and_then(|a| field_score(a, needle)).map(|s| s * 7),
        track.album.as_deref().and_then(|a| field_score(a, needle)).map(|s| s * 7),
    ];
    if let Some(best) = by_field.into_iter().flatten().max() {
        return Some(best);
    }
    let mut words = needle.split_whitespace().peekable();
    if words.peek().is_none() {
        return None;
    }
    let haystack = format!(
        "{} {} {}",
        track.title.to_lowercase(),
        track.artist.as_deref().unwrap_or("").to_lowercase(),
        track.album.as_deref().unwrap_or("").to_lowercase()
    );
    words.all(|w| haystack.contains(w)).then_some(15)
}

/// "Manual order": tracks follow the stored `order`; tracks that are not in it
/// (new files) go to the end, sorted by path.
pub fn apply_manual_order(tracks: &mut [Track], order: &[PathBuf]) {
    let pos: HashMap<&PathBuf, usize> = order.iter().enumerate().map(|(i, p)| (p, i)).collect();
    tracks.sort_by(|a, b| {
        let ia = pos.get(&a.path).copied().unwrap_or(usize::MAX);
        let ib = pos.get(&b.path).copied().unwrap_or(usize::MAX);
        ia.cmp(&ib).then_with(|| a.path.cmp(&b.path))
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
    fn search_ranks_closer_matches_higher() {
        let mk = |title: &str, album: Option<&str>| Track {
            title: title.into(),
            album: album.map(Into::into),
            ..Default::default()
        };
        let exact = mk("Wa", None);
        let prefix = mk("Watra", None);
        let word = mk("Big Wave", None);
        let inside = mk("Hawaii", None);
        let by_album = mk("Other", Some("Watra"));
        let none = mk("Zzz", None);
        let s = |t: &Track| search_score(t, "wa");
        assert!(s(&exact) > s(&prefix));
        assert!(s(&prefix) > s(&word));
        assert!(s(&word) > s(&inside));
        assert!(s(&by_album).is_some() && s(&by_album) < s(&prefix));
        assert_eq!(s(&none), None);
    }

    #[test]
    fn search_with_several_words_matches_across_fields() {
        let t = Track { title: "Song".into(), artist: Some("The Band".into()), ..Default::default() };
        assert!(search_score(&t, "band song").is_some());
        assert!(search_score(&t, "band zzz").is_none());
    }

    #[test]
    fn manual_order_is_restored_and_new_files_go_last() {
        let mk = |p: &str| Track { title: p.into(), path: PathBuf::from(p), ..Default::default() };
        let mut v = vec![mk("a"), mk("b"), mk("c"), mk("d")];
        apply_manual_order(&mut v, &[PathBuf::from("c"), PathBuf::from("a"), PathBuf::from("b")]);
        assert_eq!(v.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["c", "a", "b", "d"]);
    }

    #[test]
    fn equal_keys_fall_back_to_title_in_both_directions() {
        let mut v = vec![t("b"), t("a"), t("c")];
        sort_tracks(&mut v, SortState { key: SortKey::Artist, descending: true });
        assert_eq!(v.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
    }
}
