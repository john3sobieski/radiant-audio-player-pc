//! Lyrics: a `.lrc` / `.txt` file next to the track, else the lyrics embedded in the tags.
//! `.lrc` lines look like `[01:23.45]text` (several stamps per line and `[offset:+500]` work).
//! Files are read as UTF-8 (a BOM is skipped; other encodings show replacement characters).

use crate::models::Track;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LyricLine {
    /// Start time; `None` for plain (not synchronized) lyrics.
    pub time_ms: Option<u64>,
    pub text: String,
}

/// True when every line has a time (so the screen can follow the playback position).
pub fn is_synced(lines: &[LyricLine]) -> bool {
    !lines.is_empty() && lines.iter().all(|l| l.time_ms.is_some())
}

/// Loads the lyrics of a track (blocking file reads: call it off the UI thread).
pub fn load(track: &Track) -> Vec<LyricLine> {
    if let Some(path) = &track.lyrics_path {
        if let Ok(bytes) = std::fs::read(path) {
            let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(&bytes);
            let lines = parse(&String::from_utf8_lossy(bytes));
            if !lines.is_empty() {
                return lines;
            }
        }
    }
    if track.tag_lyrics {
        if let Some(text) = crate::library::read_tag_lyrics(&track.path) {
            return parse(&text);
        }
    }
    Vec::new()
}

/// Synchronized lyrics (sorted by time) if any line has a time stamp, otherwise plain lines.
pub fn parse(text: &str) -> Vec<LyricLine> {
    let mut offset_ms: i64 = 0;
    let mut timed: Vec<(i64, String)> = Vec::new();
    let mut plain: Vec<String> = Vec::new();

    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps: Vec<i64> = Vec::new();
        let mut meta = false;
        while let Some(inner) = rest.strip_prefix('[') {
            let Some(end) = inner.find(']') else { break };
            let tag = &inner[..end];
            if let Some(ms) = parse_stamp(tag) {
                stamps.push(ms as i64);
            } else if let Some(value) = tag.strip_prefix("offset:") {
                offset_ms = value.trim().parse().unwrap_or(0);
                meta = true;
            } else if is_meta_tag(tag) {
                meta = true;
            } else {
                break;
            }
            rest = inner[end + 1..].trim_start();
        }
        if meta && stamps.is_empty() {
            continue; // [ar:], [ti:], [offset:] ...
        }
        if stamps.is_empty() {
            plain.push(rest.to_string());
        } else {
            for ms in stamps {
                timed.push((ms, rest.trim().to_string()));
            }
        }
    }

    if !timed.is_empty() {
        timed.sort_by_key(|(ms, _)| *ms); // stable: equal times keep file order
        return timed
            .into_iter()
            // A positive [offset:] means the lyrics should appear earlier.
            .map(|(ms, text)| LyricLine { time_ms: Some((ms - offset_ms).max(0) as u64), text })
            .collect();
    }
    while plain.last().is_some_and(|l| l.is_empty()) {
        plain.pop();
    }
    while plain.first().is_some_and(|l| l.is_empty()) {
        plain.remove(0);
    }
    plain.into_iter().map(|text| LyricLine { time_ms: None, text }).collect()
}

/// `mm:ss`, `mm:ss.x`, `mm:ss.xx`, `mm:ss.xxx` -> milliseconds.
fn parse_stamp(tag: &str) -> Option<u64> {
    let (minutes, rest) = tag.split_once(':')?;
    let minutes: u64 = minutes.trim().parse().ok()?;
    let (seconds, fraction) = match rest.split_once(|c: char| c == '.' || c == ':') {
        Some((s, f)) => (s, f),
        None => (rest, ""),
    };
    let seconds: u64 = seconds.trim().parse().ok()?;
    let millis = if fraction.is_empty() {
        0
    } else {
        let digits: String = fraction.trim().chars().take(3).collect();
        let n: u64 = digits.parse().ok()?;
        match digits.len() {
            1 => n * 100,
            2 => n * 10,
            _ => n,
        }
    };
    Some((minutes * 60 + seconds) * 1000 + millis)
}

fn is_meta_tag(tag: &str) -> bool {
    tag.split_once(':')
        .is_some_and(|(key, _)| !key.is_empty() && key.chars().all(|c| c.is_ascii_alphabetic()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_synced_lines_with_several_stamps_and_meta() {
        let text = "[ar:Someone]\n[ti:Song]\n[00:12.50]second\n[00:01.5][00:30]first and again\n";
        let lines = parse(text);
        assert!(is_synced(&lines));
        assert_eq!(lines[0], LyricLine { time_ms: Some(1500), text: "first and again".into() });
        assert_eq!(lines[1], LyricLine { time_ms: Some(12_500), text: "second".into() });
        assert_eq!(lines[2].time_ms, Some(30_000));
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn offset_shifts_times() {
        let lines = parse("[offset:500]\n[00:02.00]a\n");
        assert_eq!(lines[0].time_ms, Some(1500));
    }

    #[test]
    fn plain_text_stays_plain() {
        let lines = parse("\nline one\n\nline two\n\n");
        assert!(!is_synced(&lines));
        assert_eq!(lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(), ["line one", "", "line two"]);
    }

    #[test]
    fn bracket_text_that_is_not_a_tag_is_kept() {
        let lines = parse("[Chorus] la la\n");
        assert_eq!(lines[0].text, "[Chorus] la la");
    }
}
