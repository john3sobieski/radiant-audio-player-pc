//! Persistent app settings (serde + JSON).

use radiant_dsp::AudioFxSettings;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

const APP_DIR: &str = "Radiant";
const SETTINGS_FILE: &str = "settings.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepeatModeSetting {
    #[default]
    Off,
    All,
    One,
}

/// How the 12-band equalizer curve is chosen. In `PerTrack` mode the speed, bass, treble and subsonic
/// settings are stored per track too; normalize and mono always stay global.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EqMode {
    /// The equalizer is bypassed; all curves stay stored.
    Off,
    /// One curve (`fx.eq_gains_db`) for every track.
    #[default]
    Global,
    /// Every track has its own curve, speed, bass, treble and subsonic, stored by file path;
    /// a track without an entry plays flat, at normal speed and without bass / treble / subsonic.
    PerTrack,
}

impl EqMode {
    pub fn to_index(self) -> i32 {
        match self {
            EqMode::Off => 0,
            EqMode::Global => 1,
            EqMode::PerTrack => 2,
        }
    }

    pub fn from_index(i: i32) -> Self {
        match i {
            0 => EqMode::Off,
            2 => EqMode::PerTrack,
            _ => EqMode::Global,
        }
    }
}

/// Key of a track in `AppSettings::track_eq`.
pub fn eq_key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// The settings that follow a track in `EqMode::PerTrack` (besides its equalizer curve).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackSound {
    /// Speed slider position, -2 ..= +2 in steps of 0.1.
    pub speed_t: f32,
    pub bass_db: f32,
    pub treble_db: f32,
    pub subsonic_percent: f32,
}

/// How many own colors can be saved as presets.
pub const MAX_SAVED_COLORS: usize = 12;

/// Interface language that follows the Windows display language (falls back to English).
pub const LANGUAGE_SYSTEM: &str = "system";

/// The ten interface languages, in the order of the picker (must match `radiant-ui/src/i18n.rs` and `radiant-ui/lang/`).
pub const LANGUAGE_CODES: [&str; 10] = ["en", "de", "ru", "uk", "zh", "ja", "fr", "cs", "es", "pl"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub music_folder: Option<PathBuf>,
    pub volume: f32,
    pub shuffle: bool,
    pub repeat_mode: RepeatModeSetting,
    /// Playback speed slider position, -2 ..= +2 in steps of 0.1.
    pub speed_t: f32,
    /// Sound effects. `fx.eq_gains_db` is the curve of the "one curve for all tracks" mode.
    pub fx: AudioFxSettings,
    pub eq_mode: EqMode,
    /// Own equalizer curves by track path (used in `EqMode::PerTrack`); flat curves are not stored.
    pub track_eq: BTreeMap<String, [f32; 12]>,
    /// Own speed / bass / treble / subsonic by track path (used in `EqMode::PerTrack`); default values are not stored.
    pub track_sound: BTreeMap<String, TrackSound>,
    /// Track order set by hand (dragging) in the "Manual order" sort mode.
    pub manual_order: Vec<PathBuf>,
    /// Open section: 0 = Tracks, 1 = Albums, 2 = Playlists, 3 = Queues.
    pub section: u8,
    /// Open queue inside the Queues section, 0..=3.
    pub queue_index: u8,
    /// Sort of the Playlists page: 0 = date created, 1 = name, 2 = date changed, 3 = track count, 4 = manual order.
    pub playlist_sort: u8,
    pub playlist_sort_desc: bool,
    /// Sort of the Albums grid: 0 = manual order, 1 = name, 2 = artist, 3 = duration.
    pub album_sort: u8,
    pub album_sort_desc: bool,
    /// Album keys in the order set by hand (dragging tiles) in the manual sort mode.
    pub album_order: Vec<String>,
    /// Color theme: 0 = System (follows Windows), 1 = Light, 2 = Dark, 3 = Black (OLED).
    pub theme: u8,
    /// The section the settings page was left on (0 = first); it opens there next time.
    pub settings_section: u8,
    /// Chosen palette per theme kind [light, dark, black]: 0..=9 = preset, 10 = own color.
    pub palettes: [u8; 3],
    /// The own accent color as [hue, saturation, value], each 0.0..=1.0 (shared by all themes).
    pub custom_hsv: [f32; 3],
    /// Own colors the user saved as presets ([hue, saturation, value]); at most `MAX_SAVED_COLORS`.
    pub saved_colors: Vec<[f32; 3]>,
    /// Interface language: `LANGUAGE_SYSTEM` ("system") or one of `LANGUAGE_CODES`.
    pub language: String,
    /// Low-power mode: no animations, no spectrum rays, sound effects (equalizer, bass, treble, subsonic,
    /// normalization) bypassed, AI stems not allowed, the engine wakes up less often. Everything else keeps working.
    pub low_power: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            music_folder: None,
            volume: 1.0,
            shuffle: false,
            repeat_mode: RepeatModeSetting::Off,
            speed_t: 0.0,
            fx: AudioFxSettings::default(),
            eq_mode: EqMode::Global,
            track_eq: BTreeMap::new(),
            track_sound: BTreeMap::new(),
            manual_order: Vec::new(),
            section: 0,
            queue_index: 0,
            playlist_sort: 0,
            playlist_sort_desc: false,
            album_sort: 1,
            album_sort_desc: false,
            album_order: Vec::new(),
            theme: 2,
            settings_section: 0,
            palettes: [0, 0, 0],
            custom_hsv: [0.75, 0.55, 1.0],
            saved_colors: Vec::new(),
            language: LANGUAGE_SYSTEM.to_string(),
            low_power: false,
        }
    }
}

impl AppSettings {
    /// The curve that is audible for `track` in the current mode (flat when off / unknown track).
    pub fn effective_eq(&self, track: Option<&Path>) -> [f32; 12] {
        match self.eq_mode {
            EqMode::Off => [0.0; 12],
            EqMode::Global => self.fx.eq_gains_db,
            EqMode::PerTrack => track
                .and_then(|t| self.track_eq.get(&eq_key(t)))
                .copied()
                .unwrap_or([0.0; 12]),
        }
    }

    /// True when the sliders and presets may change a curve now.
    pub fn eq_editable(&self, track: Option<&Path>) -> bool {
        match self.eq_mode {
            EqMode::Off => false,
            EqMode::Global => true,
            EqMode::PerTrack => track.is_some(),
        }
    }

    /// True when `track` has its own stored curve.
    pub fn has_track_eq(&self, track: Option<&Path>) -> bool {
        track.is_some_and(|t| self.track_eq.contains_key(&eq_key(t)))
    }

    /// Stores `gains` as the active curve (global, or the one of `track`). A flat per-track curve removes
    /// the entry. Returns false (nothing changed) when the mode does not allow editing.
    pub fn set_active_eq(&mut self, track: Option<&Path>, gains: [f32; 12]) -> bool {
        match self.eq_mode {
            EqMode::Off => false,
            EqMode::Global => {
                self.fx.eq_gains_db = gains;
                true
            }
            EqMode::PerTrack => {
                let Some(track) = track else { return false };
                if gains == [0.0; 12] {
                    self.track_eq.remove(&eq_key(track));
                } else {
                    self.track_eq.insert(eq_key(track), gains);
                }
                true
            }
        }
    }

    /// True when speed / bass / treble / subsonic belong to `track` (per-track mode with a known track)
    /// instead of being global.
    pub fn uses_track_sound(&self, track: Option<&Path>) -> bool {
        self.eq_mode == EqMode::PerTrack && track.is_some()
    }

    /// The speed / bass / treble / subsonic that are audible for `track` in the current mode.
    pub fn effective_sound(&self, track: Option<&Path>) -> TrackSound {
        match track {
            Some(t) if self.eq_mode == EqMode::PerTrack => {
                self.track_sound.get(&eq_key(t)).copied().unwrap_or_default()
            }
            _ => TrackSound {
                speed_t: self.speed_t,
                bass_db: self.fx.bass_db,
                treble_db: self.fx.treble_db,
                subsonic_percent: self.fx.subsonic_percent,
            },
        }
    }

    /// Stores `sound` for `track` (per-track mode) or as the global values. A default entry is dropped.
    pub fn set_sound(&mut self, track: Option<&Path>, sound: TrackSound) {
        match track {
            Some(t) if self.eq_mode == EqMode::PerTrack => {
                if sound == TrackSound::default() {
                    self.track_sound.remove(&eq_key(t));
                } else {
                    self.track_sound.insert(eq_key(t), sound);
                }
            }
            _ => {
                self.speed_t = sound.speed_t;
                self.fx.bass_db = sound.bass_db;
                self.fx.treble_db = sound.treble_db;
                self.fx.subsonic_percent = sound.subsonic_percent;
            }
        }
    }

    /// Stores the speed slider position for `track` (per-track mode) or globally.
    pub fn set_speed_for_track(&mut self, track: Option<&Path>, speed_t: f32) {
        let mut sound = self.effective_sound(track);
        sound.speed_t = speed_t;
        self.set_sound(track, sound);
    }

    /// Takes the effects that do not belong to the equalizer curve from `fx` (the engine copy of the
    /// settings may carry another curve than the one to store here). Bass, treble and subsonic go to
    /// `track` in per-track mode; normalize and mono are always global.
    pub fn set_fx_for_track(&mut self, track: Option<&Path>, fx: &AudioFxSettings) {
        self.fx.normalize = fx.normalize;
        self.fx.mono = fx.mono;
        let mut sound = self.effective_sound(track);
        sound.bass_db = fx.bass_db;
        sound.treble_db = fx.treble_db;
        sound.subsonic_percent = fx.subsonic_percent;
        self.set_sound(track, sound);
    }

    /// Same as `set_fx_for_track` without a track: everything is stored globally.
    pub fn set_fx_keep_eq(&mut self, fx: &AudioFxSettings) {
        self.set_fx_for_track(None, fx);
    }

    /// "Reset" of the sound panel: effects back to defaults (the speed stays) and the ACTIVE curve flat;
    /// curves that are not active (other tracks, or all curves while the equalizer is off) are kept.
    /// In per-track mode only the bass / treble / subsonic of `track` are reset, the global ones stay.
    pub fn reset_sound(&mut self, track: Option<&Path>) {
        let mut sound = self.effective_sound(track);
        sound.bass_db = 0.0;
        sound.treble_db = 0.0;
        sound.subsonic_percent = 0.0;
        self.set_sound(track, sound);
        self.fx.normalize = false;
        self.fx.mono = false;
        match self.eq_mode {
            EqMode::Global => self.fx.eq_gains_db = [0.0; 12],
            EqMode::Off => {}
            EqMode::PerTrack => {
                if let Some(t) = track {
                    self.track_eq.remove(&eq_key(t));
                }
            }
        }
    }
}

pub fn settings_path() -> PathBuf {
    config_root().join(APP_DIR).join(SETTINGS_FILE)
}

pub fn load() -> Result<AppSettings, String> {
    read_at(&settings_path())
}

pub fn save(settings: &AppSettings) -> Result<(), String> {
    write_at(&settings_path(), settings)
}

fn read_at(path: &Path) -> Result<AppSettings, String> {
    match fs::read_to_string(path) {
        Ok(text) => parse(&text).map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(AppSettings::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn parse(text: &str) -> Result<AppSettings, serde_json::Error> {
    let mut settings = serde_json::from_str::<AppSettings>(text)?;
    sanitize(&mut settings);
    Ok(settings)
}

/// Keeps a hand-edited or older file inside the ranges the sliders and the engine use.
fn sanitize(settings: &mut AppSettings) {
    settings.section = settings.section.min(3);
    settings.queue_index = settings.queue_index.min(3);
    settings.playlist_sort = settings.playlist_sort.min(4);
    settings.album_sort = settings.album_sort.min(3);
    settings.volume = finite(settings.volume, 1.0).clamp(0.0, 1.0);
    settings.theme = settings.theme.min(3);
    settings.settings_section = settings.settings_section.min(5); // 6 sections now; raise when one is added
    for p in settings.palettes.iter_mut() {
        *p = (*p).min(10);
    }
    settings.saved_colors.truncate(MAX_SAVED_COLORS);
    if settings.language != LANGUAGE_SYSTEM && !LANGUAGE_CODES.contains(&settings.language.as_str()) {
        settings.language = LANGUAGE_SYSTEM.to_string();
    }
    for color in settings.saved_colors.iter_mut() {
        for c in color.iter_mut() {
            *c = finite(*c, 0.0).clamp(0.0, 1.0);
        }
    }
    let defaults = [0.75, 0.55, 1.0];
    for (c, d) in settings.custom_hsv.iter_mut().zip(defaults) {
        *c = finite(*c, d).clamp(0.0, 1.0);
    }
    settings.speed_t = snap(finite(settings.speed_t, 0.0).clamp(-2.0, 2.0), 10.0);
    let fx = &mut settings.fx;
    fx.bass_db = snap(finite(fx.bass_db, 0.0).clamp(0.0, 15.0), 2.0);
    fx.treble_db = snap(finite(fx.treble_db, 0.0).clamp(0.0, 15.0), 2.0);
    fx.subsonic_percent = finite(fx.subsonic_percent, 0.0).clamp(0.0, 100.0).round();
    for gain in &mut fx.eq_gains_db {
        *gain = snap(finite(*gain, 0.0).clamp(-12.0, 12.0), 2.0);
    }
    for curve in settings.track_eq.values_mut() {
        for gain in curve.iter_mut() {
            *gain = snap(finite(*gain, 0.0).clamp(-12.0, 12.0), 2.0);
        }
    }
    settings.track_eq.retain(|_, curve| *curve != [0.0; 12]);
    for sound in settings.track_sound.values_mut() {
        sound.speed_t = snap(finite(sound.speed_t, 0.0).clamp(-2.0, 2.0), 10.0);
        sound.bass_db = snap(finite(sound.bass_db, 0.0).clamp(0.0, 15.0), 2.0);
        sound.treble_db = snap(finite(sound.treble_db, 0.0).clamp(0.0, 15.0), 2.0);
        sound.subsonic_percent = finite(sound.subsonic_percent, 0.0).clamp(0.0, 100.0).round();
    }
    settings.track_sound.retain(|_, sound| *sound != TrackSound::default());
}

fn finite(v: f32, default: f32) -> f32 {
    if v.is_finite() { v } else { default }
}

/// `steps` is how many steps fit in 1.0 (10 = 0.1, 2 = 0.5).
fn snap(v: f32, steps: f32) -> f32 {
    (v * steps).round() / steps
}

fn write_at(path: &Path, settings: &AppSettings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
    }
    let mut data = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
    data.push(b'\n');
    // Write aside first, so a crash never leaves a half-written settings file.
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    replace_file(&tmp, path)
}

pub(crate) fn replace_file(tmp: &Path, path: &Path) -> Result<(), String> {
    // Windows cannot rename over a file that already exists.
    match fs::rename(tmp, path) {
        Ok(()) => Ok(()),
        Err(_) if path.exists() => {
            fs::remove_file(path).map_err(|e| format!("{}: {e}", path.display()))?;
            fs::rename(tmp, path).map_err(|e| format!("{}: {e}", path.display()))
        }
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn config_root() -> PathBuf {
    #[cfg(windows)]
    if let Some(dir) = env::var_os("APPDATA") {
        return PathBuf::from(dir);
    }

    #[cfg(not(windows))]
    {
        if let Some(dir) = env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(dir);
        }
    }

    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".config");
    }

    PathBuf::from(".")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("radiant-settings-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sample() -> AppSettings {
        let mut s = AppSettings::default();
        s.music_folder = Some(PathBuf::from("D:/Музыка/альбом"));
        s.volume = 0.25;
        s.shuffle = true;
        s.repeat_mode = RepeatModeSetting::One;
        s.language = "uk".to_string();
        s.speed_t = -1.5;
        s.fx.normalize = true;
        s.fx.mono = true;
        s.fx.bass_db = 4.5;
        s.fx.treble_db = 1.0;
        s.fx.subsonic_percent = 30.0;
        s.fx.eq_gains_db[0] = -3.5;
        s.fx.eq_gains_db[11] = 6.0;
        s.eq_mode = EqMode::PerTrack;
        s.track_eq.insert("D:/Музыка/а.mp3".to_string(), [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, -1.0, -2.0, -3.0, -4.0, -5.0, -6.0]);
        s.manual_order = vec![PathBuf::from("D:/Музыка/б.mp3"), PathBuf::from("D:/Музыка/а.mp3")];
        s.playlist_sort = 2;
        s.playlist_sort_desc = true;
        s.album_sort = 0;
        s.album_sort_desc = true;
        s.album_order = vec!["b".to_string(), "a".to_string()];
        s
    }

    #[test]
    fn missing_file_is_default() {
        let path = test_dir("missing").join("settings.json");
        assert_eq!(read_at(&path).unwrap(), AppSettings::default());
    }

    #[test]
    fn round_trip_keeps_every_field() {
        let dir = test_dir("round");
        let path = dir.join("settings.json");
        let original = sample();
        write_at(&path, &original).unwrap();
        assert_eq!(read_at(&path).unwrap(), original);
        assert!(!path.with_extension("json.tmp").exists());
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"repeat_mode\": \"one\""));
        assert!(text.contains("Музыка"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn second_save_replaces_the_file() {
        let dir = test_dir("replace");
        let path = dir.join("settings.json");
        write_at(&path, &AppSettings::default()).unwrap();
        let mut next = AppSettings::default();
        next.volume = 0.4;
        write_at(&path, &next).unwrap();
        assert_eq!(read_at(&path).unwrap().volume, 0.4);
        assert!(!path.with_extension("json.tmp").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn partial_json_keeps_defaults() {
        let s = parse(r#"{"volume": 0.25, "future_field": true, "fx": {"mono": true}}"#).unwrap();
        assert_eq!(s.volume, 0.25);
        assert!(s.fx.mono);
        assert!(!s.fx.normalize);
        assert_eq!(s.fx.bass_db, 0.0);
        assert_eq!(s.fx.eq_gains_db, [0.0; 12]);
        assert_eq!(s.repeat_mode, RepeatModeSetting::Off);
        assert_eq!(s.music_folder, None);
        assert!((s.speed_t - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn low_power_is_off_by_default_and_remembered() {
        assert!(!AppSettings::default().low_power);
        assert!(!parse(r#"{}"#).unwrap().low_power);
        assert!(parse(r#"{"low_power": true}"#).unwrap().low_power);
        // The power section is the sixth one of the settings page.
        assert_eq!(parse(r#"{"settings_section": 5}"#).unwrap().settings_section, 5);
        assert_eq!(parse(r#"{"settings_section": 9}"#).unwrap().settings_section, 5);
    }

    #[test]
    fn language_defaults_to_system_and_rejects_unknown_codes() {
        assert_eq!(AppSettings::default().language, LANGUAGE_SYSTEM);
        assert_eq!(parse(r#"{"language": "pl"}"#).unwrap().language, "pl");
        assert_eq!(parse(r#"{"language": "klingon"}"#).unwrap().language, LANGUAGE_SYSTEM);
        assert_eq!(parse(r#"{}"#).unwrap().language, LANGUAGE_SYSTEM);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let s = parse(
            r#"{"volume": 3.0, "speed_t": 9.0, "fx": {"bass_db": 40.0, "treble_db": -2.0, "subsonic_percent": 150.0, "eq_gains_db": [100.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -100.0]}}"#,
        )
        .unwrap();
        assert_eq!(s.volume, 1.0);
        assert_eq!(s.speed_t, 2.0);
        assert_eq!(s.fx.bass_db, 15.0);
        assert_eq!(s.fx.treble_db, 0.0);
        assert_eq!(s.fx.subsonic_percent, 100.0);
        assert_eq!(s.fx.eq_gains_db[0], 12.0);
        assert_eq!(s.fx.eq_gains_db[11], -12.0);
    }

    #[test]
    fn old_file_without_eq_mode_uses_one_curve_for_all() {
        let s = parse(r#"{"fx": {"eq_gains_db": [2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]}}"#).unwrap();
        assert_eq!(s.eq_mode, EqMode::Global);
        assert!(s.track_eq.is_empty());
        assert_eq!(s.effective_eq(None)[0], 2.0);
    }

    #[test]
    fn eq_mode_is_stored_as_text() {
        let mut s = AppSettings::default();
        s.eq_mode = EqMode::PerTrack;
        let text = serde_json::to_string(&s).unwrap();
        assert!(text.contains("\"eq_mode\":\"per_track\""));
        assert_eq!(EqMode::from_index(EqMode::Off.to_index()), EqMode::Off);
        assert_eq!(EqMode::from_index(EqMode::Global.to_index()), EqMode::Global);
        assert_eq!(EqMode::from_index(EqMode::PerTrack.to_index()), EqMode::PerTrack);
    }

    #[test]
    fn modes_choose_the_curve() {
        let a = Path::new("a.mp3");
        let b = Path::new("b.mp3");
        let mut curve = [0.0_f32; 12];
        curve[3] = 4.0;
        let mut s = AppSettings::default();

        // One curve for all tracks.
        assert!(s.set_active_eq(Some(a), curve));
        assert_eq!(s.effective_eq(Some(a)), curve);
        assert_eq!(s.effective_eq(Some(b)), curve);
        assert_eq!(s.effective_eq(None), curve);

        // Off: bypassed, but nothing is thrown away and nothing can be edited.
        s.eq_mode = EqMode::Off;
        assert_eq!(s.effective_eq(Some(a)), [0.0; 12]);
        assert!(!s.eq_editable(Some(a)));
        assert!(!s.set_active_eq(Some(a), [1.0; 12]));
        assert_eq!(s.fx.eq_gains_db, curve);

        // Per track: independent curves, flat until set, needs a current track.
        s.eq_mode = EqMode::PerTrack;
        assert_eq!(s.effective_eq(Some(a)), [0.0; 12]);
        assert!(!s.eq_editable(None));
        assert!(!s.set_active_eq(None, curve));
        let mut other = [0.0_f32; 12];
        other[9] = -6.0;
        assert!(s.set_active_eq(Some(a), curve));
        assert!(s.set_active_eq(Some(b), other));
        assert_eq!(s.effective_eq(Some(a)), curve);
        assert_eq!(s.effective_eq(Some(b)), other);
        assert!(s.has_track_eq(Some(a)));
        assert_eq!(s.fx.eq_gains_db, curve, "the global curve must stay untouched");

        // A flat curve frees the entry.
        assert!(s.set_active_eq(Some(a), [0.0; 12]));
        assert!(!s.has_track_eq(Some(a)));
        assert!(s.has_track_eq(Some(b)));
    }

    #[test]
    fn set_fx_keep_eq_leaves_the_stored_curve() {
        let mut s = AppSettings::default();
        s.fx.eq_gains_db[0] = 5.0;
        let mut engine_copy = s.fx.clone();
        engine_copy.eq_gains_db = [0.0; 12]; // e.g. equalizer off
        engine_copy.bass_db = 3.0;
        engine_copy.mono = true;
        s.set_fx_keep_eq(&engine_copy);
        assert_eq!(s.fx.eq_gains_db[0], 5.0);
        assert_eq!(s.fx.bass_db, 3.0);
        assert!(s.fx.mono);
    }

    #[test]
    fn reset_flattens_only_the_active_curve() {
        let a = Path::new("a.mp3");
        let b = Path::new("b.mp3");
        let mut s = AppSettings::default();
        s.fx.bass_db = 6.0;
        s.fx.eq_gains_db[1] = 3.0;
        s.eq_mode = EqMode::PerTrack;
        s.track_eq.insert(eq_key(a), [1.0; 12]);
        s.track_eq.insert(eq_key(b), [2.0; 12]);
        s.reset_sound(Some(a));
        assert_eq!(s.fx.bass_db, 6.0, "per-track mode leaves the global bass alone");
        assert!(!s.has_track_eq(Some(a)));
        assert!(s.has_track_eq(Some(b)));
        assert_eq!(s.fx.eq_gains_db[1], 3.0, "global curve is not the active one");

        s.eq_mode = EqMode::Global;
        s.reset_sound(Some(b));
        assert_eq!(s.fx.eq_gains_db, [0.0; 12]);
        assert_eq!(s.fx.bass_db, 0.0);
        assert!(s.has_track_eq(Some(b)));
    }

    #[test]
    fn per_track_sound_is_independent_per_track() {
        let a = Path::new("a.mp3");
        let b = Path::new("b.mp3");
        let mut s = AppSettings::default();
        s.speed_t = 1.0;
        s.fx.bass_db = 4.0;
        s.eq_mode = EqMode::PerTrack;

        // A track without an entry is neutral; the global values stay untouched.
        assert_eq!(s.effective_sound(Some(a)), TrackSound::default());
        s.set_speed_for_track(Some(a), -0.5);
        let mut fx = s.fx.clone();
        fx.treble_db = 3.0;
        fx.subsonic_percent = 20.0;
        fx.mono = true;
        s.set_fx_for_track(Some(a), &fx);
        let sound = s.effective_sound(Some(a));
        assert_eq!(sound.speed_t, -0.5);
        assert_eq!(sound.treble_db, 3.0);
        assert_eq!(sound.subsonic_percent, 20.0);
        assert_eq!(sound.bass_db, 4.0, "the engine copy carried the global bass");
        assert_eq!(s.effective_sound(Some(b)), TrackSound::default());
        assert_eq!(s.speed_t, 1.0);
        assert_eq!(s.fx.treble_db, 0.0);
        assert!(s.fx.mono, "mono is always global");

        // Without a track, or in another mode, the global values are used.
        assert_eq!(s.effective_sound(None).speed_t, 1.0);
        s.eq_mode = EqMode::Global;
        assert_eq!(s.effective_sound(Some(a)).speed_t, 1.0);

        // Back to defaults frees the entry.
        s.eq_mode = EqMode::PerTrack;
        s.set_sound(Some(a), TrackSound::default());
        assert!(s.track_sound.is_empty());
    }

    #[test]
    fn track_sound_is_saved_and_clamped() {
        let mut s = AppSettings::default();
        s.eq_mode = EqMode::PerTrack;
        s.track_sound.insert(
            "a.mp3".to_string(),
            TrackSound { speed_t: 1.5, bass_db: 4.5, treble_db: 1.0, subsonic_percent: 30.0 },
        );
        let text = serde_json::to_string(&s).unwrap();
        assert_eq!(parse(&text).unwrap(), s);

        let s = parse(
            r#"{"track_sound": {"a.mp3": {"speed_t": 9.0, "bass_db": -4.0}, "b.mp3": {"speed_t": 0.0}}}"#,
        )
        .unwrap();
        assert_eq!(s.track_sound.len(), 1);
        assert_eq!(s.track_sound["a.mp3"].speed_t, 2.0);
        assert_eq!(s.track_sound["a.mp3"].bass_db, 0.0);
    }

    #[test]
    fn track_curves_are_clamped_and_flat_ones_dropped() {
        let s = parse(
            r#"{"eq_mode": "per_track", "track_eq": {"a.mp3": [99.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -99.0], "b.mp3": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]}}"#,
        )
        .unwrap();
        assert_eq!(s.eq_mode, EqMode::PerTrack);
        assert_eq!(s.track_eq.len(), 1);
        assert_eq!(s.track_eq["a.mp3"][0], 12.0);
        assert_eq!(s.track_eq["a.mp3"][11], -12.0);
    }

    #[test]
    fn non_finite_numbers_fall_back() {
        let mut s = AppSettings::default();
        s.volume = f32::NAN;
        s.speed_t = f32::INFINITY;
        s.fx.bass_db = f32::NEG_INFINITY;
        sanitize(&mut s);
        assert_eq!(s.volume, 1.0);
        assert_eq!(s.speed_t, 0.0);
        assert_eq!(s.fx.bass_db, 0.0);
    }

    #[test]
    fn broken_json_is_an_error_and_the_old_file_stays() {
        let dir = test_dir("broken");
        let path = dir.join("settings.json");
        fs::write(&path, "{not json").unwrap();
        assert!(read_at(&path).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{not json");
        let _ = fs::remove_dir_all(&dir);
    }
}
