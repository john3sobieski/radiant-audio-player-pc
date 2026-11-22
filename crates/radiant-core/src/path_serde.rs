//! Saving file paths to JSON. A plain `PathBuf` fails to serialize when the name is not valid UTF-8 (old archives
//! with names in cp1251 / cp866 on Linux), and then the whole settings / playlist / queue file cannot be saved.
//! Here a valid name is written as an ordinary string (the same text as before); an invalid one as `RAW_PREFIX` plus
//! the bytes in hex, so it comes back byte for byte. Use with `#[serde(with = "crate::path_serde::vec")]` etc.

use serde::{Deserialize, Deserializer, Serializer};
use std::path::{Path, PathBuf};

/// Marks a path that is not valid UTF-8 (a private-use character, so no real name starts with it).
#[cfg(unix)]
const RAW_PREFIX: &str = "\u{F8FF}raw:";

/// Path -> text for JSON.
pub fn to_text(path: &Path) -> String {
    if let Some(s) = path.to_str() {
        return s.to_owned();
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut out = String::from(RAW_PREFIX);
        for b in path.as_os_str().as_bytes() {
            out.push_str(&format!("{b:02x}"));
        }
        out
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy().into_owned()
    }
}

/// Text from JSON -> path (the reverse of [`to_text`]; anything that is not a valid raw form is taken as is).
pub fn from_text(text: &str) -> PathBuf {
    #[cfg(unix)]
    if let Some(hex) = text.strip_prefix(RAW_PREFIX) {
        if let Some(bytes) = decode_hex(hex) {
            use std::os::unix::ffi::OsStringExt;
            return PathBuf::from(std::ffi::OsString::from_vec(bytes));
        }
    }
    PathBuf::from(text)
}

#[cfg(unix)]
fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if hex.len() % 2 != 0 || !hex.is_ascii() {
        return None;
    }
    (0..hex.len()).step_by(2).map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok()).collect()
}

/// `Vec<PathBuf>`.
pub mod vec {
    use super::*;

    pub fn serialize<S: Serializer>(paths: &[PathBuf], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(paths.iter().map(|p| to_text(p)))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<PathBuf>, D::Error> {
        Ok(Vec::<String>::deserialize(d)?.iter().map(|t| from_text(t)).collect())
    }
}

/// `Vec<Vec<PathBuf>>`.
pub mod vecvec {
    use super::*;

    pub fn serialize<S: Serializer>(lists: &[Vec<PathBuf>], s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(lists.iter().map(|l| l.iter().map(|p| to_text(p)).collect::<Vec<_>>()))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec<PathBuf>>, D::Error> {
        Ok(Vec::<Vec<String>>::deserialize(d)?.iter().map(|l| l.iter().map(|t| from_text(t)).collect()).collect())
    }
}

/// `Option<PathBuf>`.
pub mod opt {
    use super::*;

    pub fn serialize<S: Serializer>(path: &Option<PathBuf>, s: S) -> Result<S::Ok, S::Error> {
        match path {
            Some(p) => s.serialize_some(&to_text(p)),
            None => s.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<PathBuf>, D::Error> {
        Ok(Option::<String>::deserialize(d)?.map(|t| from_text(&t)))
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    #[test]
    fn valid_names_stay_plain_text() {
        let p = PathBuf::from("/music/альбом/песня.mp3");
        assert_eq!(to_text(&p), "/music/альбом/песня.mp3");
        assert_eq!(from_text(&to_text(&p)), p);
    }

    #[test]
    fn invalid_utf8_round_trips_byte_for_byte() {
        // "/m/" + a cp1251 "п" (0xEF) that is not valid UTF-8 + ".mp3"
        let p = PathBuf::from(OsString::from_vec(vec![b'/', b'm', b'/', 0xEF, b'.', b'm', b'p', b'3']));
        assert!(p.to_str().is_none());
        let text = to_text(&p);
        assert!(text.starts_with(RAW_PREFIX));
        assert_eq!(from_text(&text), p);
    }

    #[test]
    fn broken_raw_form_is_kept_as_text() {
        let text = format!("{RAW_PREFIX}zz");
        assert_eq!(from_text(&text), PathBuf::from(&text));
    }

    #[test]
    fn json_round_trip_of_the_helpers() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct S {
            #[serde(with = "super::vec")]
            a: Vec<PathBuf>,
            #[serde(with = "super::vecvec")]
            b: Vec<Vec<PathBuf>>,
            #[serde(with = "super::opt")]
            c: Option<PathBuf>,
        }
        let bad = PathBuf::from(OsString::from_vec(vec![0xFF, 0xFE]));
        let s = S { a: vec![bad.clone(), "x.mp3".into()], b: vec![vec![bad.clone()], vec![]], c: Some(bad) };
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(serde_json::from_str::<S>(&json).unwrap(), s);
    }
}
