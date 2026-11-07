//! Lyrics: a `.lrc` / `.txt` file next to the track, else the lyrics embedded in the tags
//! (ID3 SYLT first, then USLT / Vorbis LYRICS / M4A lyrics text).
//! `.lrc` lines look like `[01:23.45]text` (several stamps per line and `[offset:+500]` work).
//! Files are read as UTF-8, UTF-16 (with BOM) or, when that fails, as Windows-1251 / Windows-1252.

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
            let lines = parse(&decode_bytes(&bytes));
            if !lines.is_empty() {
                return lines;
            }
        }
    }
    if track.tag_lyrics {
        // Synchronized lyrics frame of an MP3 (Symphonia does not expose it).
        if crate::id3::is_mp3(&track.path) {
            if let Some(entries) = crate::id3::read_sylt(&track.path) {
                let mut lines: Vec<LyricLine> = entries
                    .into_iter()
                    .map(|(ms, text)| LyricLine { time_ms: Some(ms), text })
                    .collect();
                lines.sort_by_key(|l| l.time_ms); // stable: equal times keep their order
                if !lines.is_empty() {
                    return lines;
                }
            }
        }
        if let Some(text) = crate::library::read_tag_lyrics(&track.path) {
            return parse(&text);
        }
    }
    Vec::new()
}

/// The current line starts a little early so the eye can follow it.
pub const LEAD_MS: u64 = 250;

/// Index of the line to highlight at `position_ms` (binary search over the sorted start times, with
/// `LEAD_MS` of look-ahead); -1 before the first line or without times.
pub fn current_line(times: &[u64], position_ms: u64) -> i32 {
    let at = position_ms.saturating_add(LEAD_MS);
    times.partition_point(|t| *t <= at) as i32 - 1
}

/// Text of a lyrics file or tag: UTF-8 (BOM skipped), UTF-16 with BOM, else a legacy 8-bit code page
/// (Windows-1251 for mostly Cyrillic text, otherwise Windows-1252).
pub fn decode_bytes(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return decode_utf16(bytes, false);
    }
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => decode_legacy(bytes),
    }
}

/// UTF-16 text; a BOM (FF FE / FE FF) decides the byte order, `default_be` is used without one.
pub(crate) fn decode_utf16(raw: &[u8], default_be: bool) -> String {
    let (be, body) = match raw {
        [0xFF, 0xFE, rest @ ..] => (false, rest),
        [0xFE, 0xFF, rest @ ..] => (true, rest),
        _ => (default_be, raw),
    };
    let units: Vec<u16> = body
        .chunks_exact(2)
        .map(|c| if be { u16::from_be_bytes([c[0], c[1]]) } else { u16::from_le_bytes([c[0], c[1]]) })
        .collect();
    String::from_utf16_lossy(&units)
}

/// Cyrillic text in an 8-bit code page has almost only high bytes among its letters, Western text
/// (accents only) has few of them.
fn decode_legacy(bytes: &[u8]) -> String {
    let high = bytes.iter().filter(|b| **b >= 0x80).count();
    let letters = bytes.iter().filter(|b| b.is_ascii_alphabetic() || **b >= 0x80).count();
    let cyrillic = letters > 0 && high * 100 / letters >= 30;
    bytes.iter().map(|b| if cyrillic { cp1251(*b) } else { cp1252(*b) }).collect()
}

fn cp1251(b: u8) -> char {
    // 0x80..=0xBF; 0xC0..=0xFF are the letters А..я in order.
    const HIGH: [u16; 64] = [
        0x0402, 0x0403, 0x201A, 0x0453, 0x201E, 0x2026, 0x2020, 0x2021, 0x20AC, 0x2030, 0x0409, 0x2039, 0x040A, 0x040C,
        0x040B, 0x040F, 0x0452, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0xFFFD, 0x2122, 0x0459, 0x203A,
        0x045A, 0x045C, 0x045B, 0x045F, 0x00A0, 0x040E, 0x045E, 0x0408, 0x00A4, 0x0490, 0x00A6, 0x00A7, 0x0401, 0x00A9,
        0x0404, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x0407, 0x00B0, 0x00B1, 0x0406, 0x0456, 0x0491, 0x00B5, 0x00B6, 0x00B7,
        0x0451, 0x2116, 0x0454, 0x00BB, 0x0458, 0x0405, 0x0455, 0x0457,
    ];
    match b {
        0..=0x7F => b as char,
        0x80..=0xBF => char::from_u32(u32::from(HIGH[usize::from(b - 0x80)])).unwrap_or('\u{FFFD}'),
        _ => char::from_u32(0x0410 + u32::from(b - 0xC0)).unwrap_or('\u{FFFD}'),
    }
}

fn cp1252(b: u8) -> char {
    // 0x80..=0x9F differ from Latin-1; everything else is the same code point as the byte.
    const MID: [u16; 32] = [
        0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0xFFFD,
        0x017D, 0xFFFD, 0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A,
        0x0153, 0xFFFD, 0x017E, 0x0178,
    ];
    match b {
        0x80..=0x9F => char::from_u32(u32::from(MID[usize::from(b - 0x80)])).unwrap_or('\u{FFFD}'),
        _ => b as char,
    }
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
    fn current_line_follows_the_position_with_a_lead() {
        let times = [1000, 5000, 9000];
        assert_eq!(current_line(&times, 0), -1);
        assert_eq!(current_line(&times, 1000 - LEAD_MS - 1), -1);
        assert_eq!(current_line(&times, 1000 - LEAD_MS), 0); // the line lights up a little early
        assert_eq!(current_line(&times, 4000), 0);
        assert_eq!(current_line(&times, 5000), 1);
        assert_eq!(current_line(&times, 999_999), 2);
        assert_eq!(current_line(&[], 5000), -1);
    }

    #[test]
    fn decodes_utf8_utf16_and_legacy_files() {
        assert_eq!(decode_bytes(b"\xEF\xBB\xBFabc"), "abc");
        assert_eq!(decode_bytes("Привет".as_bytes()), "Привет");
        assert_eq!(decode_bytes(&[0xFF, 0xFE, b'a', 0, 0x1F, 0x04]), "a\u{41F}");
        assert_eq!(decode_bytes(&[0xFE, 0xFF, 0, b'a', 0x04, 0x1F]), "a\u{41F}");
        // "Привет, мир" in Windows-1251.
        let cp1251_text = [0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2, b',', b' ', 0xEC, 0xE8, 0xF0];
        assert_eq!(decode_bytes(&cp1251_text), "Привет, мир");
        // "café" in Windows-1252 is mostly ASCII, so it is not taken for Cyrillic.
        assert_eq!(decode_bytes(&[b'c', b'a', b'f', 0xE9]), "café");
    }

    #[test]
    fn bracket_text_that_is_not_a_tag_is_kept() {
        let lines = parse("[Chorus] la la\n");
        assert_eq!(lines[0].text, "[Chorus] la la");
    }
}
