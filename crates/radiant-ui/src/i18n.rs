//! Interface translations (README section 7, PROGRESS stage 8).
//!
//! One source of truth: gettext `.po` files in `crates/radiant-ui/lang/<code>/LC_MESSAGES/radiant-ui.po`
//! (keys = the English texts of the interface; the `en` file is the identity).
//! * Texts written in `main.slint` use `@tr(...)`; Slint bundles the same files into the binary (see `build.rs`)
//!   and switches them at runtime with `slint::select_bundled_translation`.
//! * Texts made in Rust (status line, dialogs, album / playlist captions, the Info tab, preset names) go through
//!   [`tr`], [`tr_fmt`] and [`tr_n`] below, which read the very same files (embedded with `include_str!`).
//!
//! "System" follows the Windows display language and falls back to English. Switching needs no restart: call
//! [`set_language`] and then rebuild the Rust-made texts (see `apply_language` in `main.rs`).

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::{OnceLock, RwLock};

/// One interface language.
pub struct Language {
    /// Directory name in `lang/` and value stored in `settings.json` (`language`).
    pub code: &'static str,
    po: &'static str,
}

macro_rules! lang {
    ($code:literal) => {
        Language {
            code: $code,
            po: include_str!(concat!("../lang/", $code, "/LC_MESSAGES/radiant-ui.po")),
        }
    };
}

/// The ten languages in the order of the picker (their own names are in `SettingsPage` in `main.slint`:
/// English, Deutsch, Русский, Українська, 中文, 日本語, Français, Čeština, Español, Polski) (index in the UI = position + 1; 0 is "System").
/// Keep in step with `LANGUAGE_CODES` in `radiant-core/src/settings.rs`.
pub static LANGUAGES: [Language; 10] = [
    lang!("en"),
    lang!("de"),
    lang!("ru"),
    lang!("uk"),
    lang!("zh"),
    lang!("ja"),
    lang!("fr"),
    lang!("cs"),
    lang!("es"),
    lang!("pl"),
];

const SYSTEM: &str = "system";

struct State {
    code: &'static str,
    /// English text -> translated forms (one form, or one per plural form).
    table: HashMap<String, Vec<String>>,
}

fn state() -> &'static RwLock<State> {
    static STATE: OnceLock<RwLock<State>> = OnceLock::new();
    STATE.get_or_init(|| RwLock::new(State { code: "en", table: HashMap::new() }))
}

/// Picker position (0 = System, 1..=10 = `LANGUAGES`) -> value for `settings.json`.
pub fn index_to_setting(index: i32) -> &'static str {
    match usize::try_from(index - 1).ok().and_then(|i| LANGUAGES.get(i)) {
        Some(l) => l.code,
        None => SYSTEM,
    }
}

/// Value from `settings.json` -> picker position.
pub fn setting_to_index(setting: &str) -> i32 {
    LANGUAGES.iter().position(|l| l.code == setting).map_or(0, |i| i as i32 + 1)
}

/// The language that "System" means right now.
pub fn system_code() -> &'static str {
    let tag = os_language_tag();
    LANGUAGES.iter().map(|l| l.code).find(|c| *c == tag).unwrap_or("en")
}

/// Windows display language (primary language id of the user's UI language).
#[cfg(windows)]
fn os_language_tag() -> String {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultUILanguage() -> u16;
    }
    // SAFETY: plain Win32 call without arguments.
    let id = unsafe { GetUserDefaultUILanguage() } & 0x3ff;
    match id {
        0x09 => "en",
        0x07 => "de",
        0x19 => "ru",
        0x22 => "uk",
        0x04 => "zh",
        0x11 => "ja",
        0x0c => "fr",
        0x05 => "cs",
        0x0a => "es",
        0x15 => "pl",
        _ => "en",
    }
    .to_string()
}

/// Other systems (used for development): the usual locale variables, "de_DE.UTF-8" -> "de".
#[cfg(not(windows))]
fn os_language_tag() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            let tag: String = value.chars().take_while(|c| c.is_ascii_alphabetic()).collect::<String>().to_lowercase();
            if tag.len() >= 2 && tag != "posix" {
                return tag;
            }
        }
    }
    "en".to_string()
}

/// Resolves a `settings.json` value ("system" or a code) to a language code.
pub fn resolve(setting: &str) -> &'static str {
    if setting == SYSTEM {
        return system_code();
    }
    LANGUAGES.iter().map(|l| l.code).find(|c| *c == setting).unwrap_or("en")
}

/// Switches the Rust-side texts to `setting` ("system" or a code) and returns the language code that is now
/// active. The caller also has to tell Slint (`slint::select_bundled_translation(code)`) and rebuild its texts.
pub fn set_language(setting: &str) -> &'static str {
    let code = resolve(setting);
    // English is the text in the source: no table needed.
    let table = if code == "en" {
        HashMap::new()
    } else {
        LANGUAGES.iter().find(|l| l.code == code).map(|l| parse_po(l.po)).unwrap_or_default()
    };
    let mut guard = state().write().unwrap_or_else(|e| e.into_inner());
    *guard = State { code, table };
    code
}

/// Translated text for the English `key` (the key itself when there is no translation).
pub fn tr(key: &str) -> String {
    let guard = state().read().unwrap_or_else(|e| e.into_inner());
    guard
        .table
        .get(key)
        .and_then(|forms| forms.first())
        .filter(|t| !t.is_empty())
        .cloned()
        .unwrap_or_else(|| key.to_string())
}

/// Like [`tr`], then every `{}` is replaced by the next argument.
pub fn tr_fmt(key: &str, args: &[&dyn Display]) -> String {
    fill(&tr(key), args)
}

/// Plural text: `singular` is the key ("{n} track"), `plural` the English plural ("{n} tracks"); `{n}` becomes `n`.
pub fn tr_n(singular: &str, plural: &str, n: usize) -> String {
    let guard = state().read().unwrap_or_else(|e| e.into_inner());
    let form = plural_index(guard.code, n as u64);
    guard
        .table
        .get(singular)
        .and_then(|forms| forms.get(form))
        .filter(|t| !t.is_empty())
        .map(String::as_str)
        .unwrap_or(if n == 1 { singular } else { plural })
        .replace("{n}", &n.to_string())
}

fn fill(text: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut rest = text;
    let mut next = 0;
    while let Some(pos) = rest.find("{}") {
        out.push_str(&rest[..pos]);
        if let Some(arg) = args.get(next) {
            out.push_str(&arg.to_string());
        }
        next += 1;
        rest = &rest[pos + 2..];
    }
    out.push_str(rest);
    out
}

/// Plural form number, the same rules as the `Plural-Forms` headers of the `.po` files.
fn plural_index(code: &str, n: u64) -> usize {
    let (m10, m100) = (n % 10, n % 100);
    match code {
        "zh" | "ja" => 0,
        "fr" => usize::from(n > 1),
        "ru" | "uk" => {
            if m10 == 1 && m100 != 11 {
                0
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                1
            } else {
                2
            }
        }
        "cs" => {
            if n == 1 {
                0
            } else if (2..=4).contains(&n) {
                1
            } else {
                2
            }
        }
        "pl" => {
            if n == 1 {
                0
            } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
                1
            } else {
                2
            }
        }
        _ => usize::from(n != 1),
    }
}

// ---- minimal .po reader (msgid / msgid_plural / msgstr / msgstr[N], multi-line strings, comments) ----

fn unquote(text: &str) -> String {
    let text = text.trim();
    let inner = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')).unwrap_or(text);
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

fn parse_po(source: &str) -> HashMap<String, Vec<String>> {
    #[derive(Clone, Copy)]
    enum Field {
        None,
        Context,
        Id,
        Plural,
        Str(usize),
    }

    fn flush(map: &mut HashMap<String, Vec<String>>, id: &mut String, forms: &mut Vec<String>) {
        // The header entry has an empty msgid; untranslated entries (empty msgstr) fall back to English.
        if !id.is_empty() && forms.iter().any(|f| !f.is_empty()) {
            map.insert(std::mem::take(id), std::mem::take(forms));
        }
        id.clear();
        forms.clear();
    }

    let mut map = HashMap::new();
    let mut id = String::new();
    let mut forms: Vec<String> = Vec::new();
    let mut field = Field::None;
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with("msgctxt") {
            flush(&mut map, &mut id, &mut forms);
            field = Field::Context;
        } else if line.starts_with("msgid_plural") {
            field = Field::Plural;
        } else if let Some(rest) = line.strip_prefix("msgid") {
            flush(&mut map, &mut id, &mut forms);
            id = unquote(rest);
            field = Field::Id;
        } else if let Some(rest) = line.strip_prefix("msgstr[") {
            let (number, tail) = rest.split_once(']').unwrap_or(("0", rest));
            let n: usize = number.trim().parse().unwrap_or(0);
            if forms.len() <= n {
                forms.resize(n + 1, String::new());
            }
            forms[n] = unquote(tail);
            field = Field::Str(n);
        } else if let Some(rest) = line.strip_prefix("msgstr") {
            forms = vec![unquote(rest)];
            field = Field::Str(0);
        } else if line.starts_with('"') {
            match field {
                Field::Id => id.push_str(&unquote(line)),
                Field::Str(n) => {
                    if let Some(form) = forms.get_mut(n) {
                        form.push_str(&unquote(line));
                    }
                }
                Field::None | Field::Context | Field::Plural => {}
            }
        }
    }
    flush(&mut map, &mut id, &mut forms);
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_plural_and_multiline_entries() {
        let po = "msgid \"\"\nmsgstr \"Language: ru\\n\"\n\n# comment\nmsgid \"Play\"\nmsgstr \"Играть\"\n\n\
                  msgid \"{n} track\"\nmsgid_plural \"{n} tracks\"\nmsgstr[0] \"{n} трек\"\nmsgstr[1] \"{n} трека\"\nmsgstr[2] \"{n} треков\"\n\n\
                  msgid \"Long\"\nmsgstr \"\"\n\"one \"\n\"two \\\"q\\\"\"\n\nmsgid \"Missing\"\nmsgstr \"\"\n";
        let map = parse_po(po);
        assert_eq!(map["Play"], vec!["Играть"]);
        assert_eq!(map["{n} track"].len(), 3);
        assert_eq!(map["Long"][0], "one two \"q\"");
        assert!(!map.contains_key("Missing"));
        assert!(!map.contains_key(""));
    }

    #[test]
    fn slavic_plural_forms() {
        for code in ["ru", "uk"] {
            assert_eq!(plural_index(code, 1), 0);
            assert_eq!(plural_index(code, 21), 0);
            assert_eq!(plural_index(code, 2), 1);
            assert_eq!(plural_index(code, 24), 1);
            assert_eq!(plural_index(code, 5), 2);
            assert_eq!(plural_index(code, 11), 2);
            assert_eq!(plural_index(code, 12), 2);
            assert_eq!(plural_index(code, 0), 2);
        }
        assert_eq!(plural_index("pl", 22), 1);
        assert_eq!(plural_index("pl", 12), 2);
        assert_eq!(plural_index("cs", 3), 1);
        assert_eq!(plural_index("cs", 5), 2);
        assert_eq!(plural_index("fr", 0), 0);
        assert_eq!(plural_index("fr", 2), 1);
        assert_eq!(plural_index("zh", 7), 0);
    }

    #[test]
    fn fill_replaces_placeholders_in_order() {
        assert_eq!(fill("Queue {} of {}", &[&1, &4]), "Queue 1 of 4");
        assert_eq!(fill("no placeholders", &[&1]), "no placeholders");
    }

    #[test]
    fn settings_values_map_to_picker_positions() {
        assert_eq!(setting_to_index("system"), 0);
        assert_eq!(setting_to_index("en"), 1);
        assert_eq!(setting_to_index("pl"), 10);
        assert_eq!(index_to_setting(0), "system");
        assert_eq!(index_to_setting(3), "ru");
        assert_eq!(index_to_setting(99), "system");
        assert_eq!(resolve("nonsense"), "en");
    }

    /// Every language file has exactly the keys of the English one (so nothing is left untranslated by accident).
    #[test]
    fn every_language_has_every_key() {
        let english = parse_po(LANGUAGES[0].po);
        assert!(english.len() > 150);
        for l in LANGUAGES.iter() {
            let table = parse_po(l.po);
            for key in english.keys() {
                assert!(table.contains_key(key), "{}: missing {key:?}", l.code);
            }
            assert_eq!(table.len(), english.len(), "{}: extra keys", l.code);
        }
    }

    #[test]
    fn languages_translate_and_fall_back() {
        let code = set_language("ru");
        assert_eq!(code, "ru");
        assert_eq!(tr("Play"), "Играть");
        assert_eq!(tr("text that is not in the files"), "text that is not in the files");
        assert_eq!(tr_n("{n} track", "{n} tracks", 1), "1 трек");
        assert_eq!(tr_n("{n} track", "{n} tracks", 3), "3 трека");
        assert_eq!(tr_n("{n} track", "{n} tracks", 7), "7 треков");
        set_language("en");
        assert_eq!(tr_n("{n} track", "{n} tracks", 2), "2 tracks");
    }
}
