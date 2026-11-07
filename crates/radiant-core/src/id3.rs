//! Minimal ID3v2 reader for the synchronized lyrics frame (SYLT, `SLT` in ID3v2.2), which Symphonia
//! does not expose. Only what lyrics need: the tag at the start of an MP3 file, the first SYLT frame
//! with millisecond stamps.

use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Tags bigger than this (cover art included) are not read.
const MAX_TAG_BYTES: usize = 16 * 1024 * 1024;

pub fn is_mp3(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("mp3"))
}

/// `(start in ms, text)` lines of the synchronized lyrics stored in the tags of an MP3 file.
/// Blocking file read: call it off the UI thread.
pub fn read_sylt(path: &Path) -> Option<Vec<(u64, String)>> {
    let mut file = File::open(path).ok()?;
    let mut head = [0u8; 10];
    file.read_exact(&mut head).ok()?;
    if &head[..3] != b"ID3" {
        return None;
    }
    let size = synchsafe(&head[6..10])?;
    if size == 0 || size > MAX_TAG_BYTES {
        return None;
    }
    let mut bytes = head.to_vec();
    bytes.resize(10 + size, 0);
    file.read_exact(&mut bytes[10..]).ok()?;
    sylt_from_tag(&bytes)
}

/// Same as `read_sylt` for a whole tag (10-byte header + body) already in memory.
pub fn sylt_from_tag(bytes: &[u8]) -> Option<Vec<(u64, String)>> {
    if bytes.len() < 10 || &bytes[..3] != b"ID3" {
        return None;
    }
    let major = bytes[3];
    if !(2..=4).contains(&major) {
        return None;
    }
    let flags = bytes[5];
    let size = synchsafe(&bytes[6..10])?;
    let end = (10 + size).min(bytes.len());
    let mut body = bytes[10..end].to_vec();
    // Tag-wide unsynchronisation: ID3v2.3 applies it to the whole body, ID3v2.4 to every frame.
    let unsync_frames = major == 4 && flags & 0x80 != 0;
    if major < 4 && flags & 0x80 != 0 {
        body = deunsync(&body);
    }

    let mut pos = 0usize;
    if flags & 0x40 != 0 && major >= 3 {
        // Extended header: skip it.
        let n = body.get(..4)?;
        pos = if major == 3 { 4 + be(n) as usize } else { synchsafe(n)? };
    }
    let (id_len, header_len) = if major == 2 { (3, 6) } else { (4, 10) };
    while pos + header_len <= body.len() {
        let id = &body[pos..pos + id_len];
        if id[0] == 0 {
            break; // padding
        }
        let size = match major {
            2 => be(&body[pos + 3..pos + 6]) as usize,
            3 => be(&body[pos + 4..pos + 8]) as usize,
            _ => match synchsafe(&body[pos + 4..pos + 8]) {
                Some(n) => n,
                None => break,
            },
        };
        let frame_flags = if major == 2 { 0 } else { be(&body[pos + 8..pos + 10]) as u16 };
        let start = pos + header_len;
        let stop = start + size;
        if stop > body.len() {
            break;
        }
        pos = stop;
        if id == b"SYLT" || id == b"SLT" {
            if let Some(entries) = parse_sylt_frame(&body[start..stop], major, frame_flags, unsync_frames) {
                let lines = assemble(entries);
                if !lines.is_empty() {
                    return Some(lines);
                }
            }
        }
    }
    None
}

/// Payload of one SYLT frame -> raw `(ms, text)` entries. Only lyrics with millisecond stamps are used
/// (stamps in MPEG frames would need the frame rate).
fn parse_sylt_frame(frame: &[u8], major: u8, flags: u16, unsync_frames: bool) -> Option<Vec<(u64, String)>> {
    let mut data = frame.to_vec();
    let mut unsync = unsync_frames;
    match major {
        3 => {
            if flags & 0x00C0 != 0 {
                return None; // compressed or encrypted
            }
            if flags & 0x0020 != 0 {
                data = data.get(1..)?.to_vec(); // group id
            }
        }
        4 => {
            if flags & 0x000C != 0 {
                return None; // compressed or encrypted
            }
            if flags & 0x0040 != 0 {
                data = data.get(1..)?.to_vec(); // group id
            }
            if flags & 0x0001 != 0 {
                data = data.get(4..)?.to_vec(); // data length indicator
            }
            unsync |= flags & 0x0002 != 0;
        }
        _ => {}
    }
    if unsync {
        data = deunsync(&data);
    }

    if data.len() < 6 {
        return None;
    }
    let encoding = data[0];
    let time_format = data[4];
    let content_type = data[5];
    if time_format != 2 || content_type > 1 {
        return None; // 0 = other, 1 = lyrics; the rest are chords, events ...
    }
    let (_, mut pos) = read_string(&data, 6, encoding)?; // content descriptor
    let mut entries = Vec::new();
    while pos < data.len() {
        let Some((text, next)) = read_string(&data, pos, encoding) else { break };
        if next + 4 > data.len() {
            break;
        }
        entries.push((u64::from(be(&data[next..next + 4])), text));
        pos = next + 4;
    }
    Some(entries)
}

/// One line per entry, or (when the entries carry line breaks, i.e. word by word sync) the words glued
/// together until a line break; a line starts at its first word.
fn assemble(entries: Vec<(u64, String)>) -> Vec<(u64, String)> {
    let word_mode = entries.iter().any(|(_, t)| t.contains('\n'));
    if !word_mode {
        return entries.into_iter().map(|(t, s)| (t, s.trim().to_string())).collect();
    }
    let mut lines: Vec<(u64, String)> = Vec::new();
    let mut current: Option<(u64, String)> = None;
    for (time, text) in entries {
        let starts = text.starts_with('\n') || text.starts_with("\r\n");
        let ends = text.ends_with('\n');
        let clean: String = text.chars().filter(|c| *c != '\n' && *c != '\r').collect();
        if starts {
            lines.extend(current.take());
        }
        match &mut current {
            Some((_, line)) => line.push_str(&clean),
            None => current = Some((time, clean)),
        }
        if ends {
            lines.extend(current.take());
        }
    }
    lines.extend(current);
    lines.into_iter().map(|(t, s)| (t, s.trim().to_string())).collect()
}

/// Reads a terminated string at `pos`; returns the text and the position after the terminator.
/// Encodings: 0 = ISO-8859-1 (often really cp1251), 1 = UTF-16 with BOM, 2 = UTF-16BE, 3 = UTF-8.
fn read_string(data: &[u8], pos: usize, encoding: u8) -> Option<(String, usize)> {
    let wide = encoding == 1 || encoding == 2;
    let rest = data.get(pos..)?;
    let (raw, next) = if wide {
        let mut i = 0;
        loop {
            if i + 1 >= rest.len() {
                return None;
            }
            if rest[i] == 0 && rest[i + 1] == 0 {
                break (&rest[..i], pos + i + 2);
            }
            i += 2;
        }
    } else {
        let i = rest.iter().position(|b| *b == 0)?;
        (&rest[..i], pos + i + 1)
    };
    let text = match encoding {
        1 => crate::lyrics::decode_utf16(raw, false),
        2 => crate::lyrics::decode_utf16(raw, true),
        3 => String::from_utf8_lossy(raw).into_owned(),
        _ => crate::lyrics::decode_bytes(raw),
    };
    Some((text, next))
}

fn be(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |acc, b| (acc << 8) | u32::from(*b))
}

/// 4 bytes with 7 bits each; `None` when a byte uses the 8th bit (not a valid size).
fn synchsafe(bytes: &[u8]) -> Option<usize> {
    if bytes.len() != 4 || bytes.iter().any(|b| b & 0x80 != 0) {
        return None;
    }
    Some(bytes.iter().fold(0usize, |acc, b| (acc << 7) | usize::from(*b)))
}

/// Undoes unsynchronisation: `FF 00` -> `FF`.
fn deunsync(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    while i < data.len() {
        out.push(data[i]);
        if data[i] == 0xFF && data.get(i + 1) == Some(&0) {
            i += 1;
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an ID3v2.3 tag with one SYLT frame (encoding 0 = Latin-1 / 3 = UTF-8, milliseconds, lyrics).
    fn tag_with_sylt(encoding: u8, entries: &[(&str, u32)]) -> Vec<u8> {
        let mut frame = vec![encoding, b'e', b'n', b'g', 2, 1, 0]; // ..., descriptor "" + terminator
        for (text, ms) in entries {
            frame.extend_from_slice(text.as_bytes());
            frame.push(0);
            frame.extend_from_slice(&ms.to_be_bytes());
        }
        let mut body = b"SYLT".to_vec();
        body.extend_from_slice(&(frame.len() as u32).to_be_bytes());
        body.extend_from_slice(&[0, 0]);
        body.extend_from_slice(&frame);
        let mut tag = b"ID3\x03\x00\x00".to_vec();
        let n = body.len();
        tag.extend_from_slice(&[(n >> 21) as u8 & 0x7F, (n >> 14) as u8 & 0x7F, (n >> 7) as u8 & 0x7F, n as u8 & 0x7F]);
        tag.extend_from_slice(&body);
        tag
    }

    #[test]
    fn reads_line_by_line_sylt() {
        let tag = tag_with_sylt(3, &[("first", 1000), ("second", 5500)]);
        assert_eq!(
            sylt_from_tag(&tag).unwrap(),
            vec![(1000, "first".to_string()), (5500, "second".to_string())]
        );
    }

    #[test]
    fn latin1_cyrillic_text_is_decoded_as_cp1251() {
        let mut tag = tag_with_sylt(0, &[("", 0)]);
        // "Привет" in cp1251 instead of the empty text: rebuild the frame by hand.
        let word = [0xCF, 0xF0, 0xE8, 0xE2, 0xE5, 0xF2];
        let mut frame = vec![0, b'r', b'u', b's', 2, 1, 0];
        frame.extend_from_slice(&word);
        frame.push(0);
        frame.extend_from_slice(&2000u32.to_be_bytes());
        let mut body = b"SYLT".to_vec();
        body.extend_from_slice(&(frame.len() as u32).to_be_bytes());
        body.extend_from_slice(&[0, 0]);
        body.extend_from_slice(&frame);
        tag.truncate(10);
        let n = body.len();
        tag[6..10].copy_from_slice(&[(n >> 21) as u8 & 0x7F, (n >> 14) as u8 & 0x7F, (n >> 7) as u8 & 0x7F, n as u8 & 0x7F]);
        tag.extend_from_slice(&body);
        assert_eq!(sylt_from_tag(&tag).unwrap(), vec![(2000, "Привет".to_string())]);
    }

    #[test]
    fn word_by_word_entries_are_joined_into_lines() {
        let entries = vec![
            (1000, "Hello ".to_string()),
            (1500, "world\n".to_string()),
            (3000, "Next ".to_string()),
            (3400, "line".to_string()),
        ];
        assert_eq!(
            assemble(entries),
            vec![(1000, "Hello world".to_string()), (3000, "Next line".to_string())]
        );
        let leading = vec![(10, "\nA ".to_string()), (20, "b".to_string()), (30, "\nC".to_string())];
        assert_eq!(assemble(leading), vec![(10, "A b".to_string()), (30, "C".to_string())]);
    }

    #[test]
    fn utf16_strings_and_frame_stamps() {
        // ID3v2.3, encoding 1, descriptor "" (BOM-less empty: 00 00), one entry "Hi" (FF FE 'H' 0 'i' 0).
        let mut frame = vec![1, b'e', b'n', b'g', 2, 1, 0, 0];
        frame.extend_from_slice(&[0xFF, 0xFE, b'H', 0, b'i', 0, 0, 0]);
        frame.extend_from_slice(&750u32.to_be_bytes());
        let entries = parse_sylt_frame(&frame, 3, 0, false).unwrap();
        assert_eq!(entries, vec![(750, "Hi".to_string())]);
    }

    #[test]
    fn mpeg_frame_stamps_and_foreign_tags_are_ignored() {
        let mut tag = tag_with_sylt(3, &[("x", 1)]);
        // Frame body starts at 10 + 10 = 20; the time stamp format byte is the 5th byte of it.
        tag[24] = 1;
        assert!(sylt_from_tag(&tag).is_none());
        assert!(sylt_from_tag(b"RIFF....").is_none());
    }

    #[test]
    fn unsynchronisation_is_undone() {
        assert_eq!(deunsync(&[1, 0xFF, 0, 2, 0xFF]), vec![1, 0xFF, 2, 0xFF]);
    }
}
