// No console window in release builds on Windows.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

//! Radiant UI binary.
//! DONE: window, folder picker, background scan, track list, playback buttons wired to the engine,
//!       sound panel (EQ, bass, treble, subsonic, normalize, mono) wired to the engine and saved,
//!       speed slider (pitch-preserving phase vocoder) wired to the engine and saved,
//!       chosen folder remembered and rescanned on start.
//! TODO: everything else in PROGRESS.md. Keep this file thin: logic belongs in the other crates.

slint::include_modules!();

use radiant_audio::{
    slider_to_speed, AudioFxSettings, Command, PlayState, Player, RepeatMode, EQ_PRESETS,
};
use radiant_core::settings::{self, AppSettings, RepeatModeSetting};
mod rays;

use radiant_core::models::{AudioInfo, Playlist, SortKey, SortState, Track, FAVORITES_ID};
use radiant_core::playlists::{self, ListEntry, PlaylistSort, PlaylistSortKey, PlaylistStore};
use slint::{Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// 83000 ms -> "1:23"
fn fmt_time(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

fn repeat_mode_to_ui(mode: RepeatModeSetting) -> i32 {
    match mode {
        RepeatModeSetting::Off => 0,
        RepeatModeSetting::All => 1,
        RepeatModeSetting::One => 2,
    }
}

fn repeat_mode_from_ui(mode: i32) -> RepeatModeSetting {
    match mode {
        1 => RepeatModeSetting::All,
        2 => RepeatModeSetting::One,
        _ => RepeatModeSetting::Off,
    }
}

fn repeat_mode_to_engine(mode: RepeatModeSetting) -> RepeatMode {
    match mode {
        RepeatModeSetting::Off => RepeatMode::Off,
        RepeatModeSetting::All => RepeatMode::All,
        RepeatModeSetting::One => RepeatMode::One,
    }
}

fn save_settings(weak: &slint::Weak<MainWindow>, settings: &RefCell<AppSettings>) {
    let snapshot = settings.borrow().clone();
    if let Err(e) = settings::save(&snapshot) {
        if let Some(ui) = weak.upgrade() {
            ui.set_status(format!("Could not save settings: {e}").into());
        }
    }
}

fn apply_settings(
    ui: &MainWindow,
    player: &Rc<Player>,
    fx: &Rc<RefCell<AudioFxSettings>>,
    eq_model: &Rc<VecModel<f32>>,
    settings: &AppSettings,
) {
    ui.set_volume(settings.volume);
    ui.set_section(settings.section as i32);
    ui.set_queue_index(settings.queue_index as i32);
    ui.set_pl_sort_mode(settings.playlist_sort as i32);
    ui.set_pl_sort_descending(settings.playlist_sort_desc);
    ui.set_shuffle(settings.shuffle);
    ui.set_repeat_mode(repeat_mode_to_ui(settings.repeat_mode));
    ui.set_speed_t(settings.speed_t);
    ui.set_fx_normalize(settings.fx.normalize);
    ui.set_fx_mono(settings.fx.mono);
    ui.set_fx_bass(settings.fx.bass_db);
    ui.set_fx_treble(settings.fx.treble_db);
    ui.set_fx_subsonic(settings.fx.subsonic_percent);
    for (i, gain) in settings.fx.eq_gains_db.iter().enumerate() {
        eq_model.set_row_data(i, *gain);
    }
    *fx.borrow_mut() = settings.fx.clone();
    player.send(Command::SetVolume(settings.volume));
    player.send(Command::SetShuffle(settings.shuffle));
    player.send(Command::SetRepeat(repeat_mode_to_engine(settings.repeat_mode)));
    player.send(Command::SetSpeed(slider_to_speed(settings.speed_t)));
    player.send(Command::SetFx(settings.fx.clone()));
}

/// Decoded cover thumbnail (RGBA8); the bytes are shared, so clones are cheap.
#[derive(Clone)]
struct CoverPixels {
    width: u32,
    height: u32,
    rgba: Arc<Vec<u8>>,
}

impl CoverPixels {
    fn to_image(&self) -> slint::Image {
        let mut buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::new(self.width, self.height);
        buffer.make_mut_bytes().copy_from_slice(&self.rgba);
        slint::Image::from_rgba8(buffer)
    }
}

/// Covers by key (`None` = looked for, none found). Bounded in `load_covers`.
static COVER_CACHE: Mutex<BTreeMap<String, Option<CoverPixels>>> = Mutex::new(BTreeMap::new());
/// Cover key of every row of the list on screen (same order as the model).
static ROW_KEYS: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Bumped on every list refresh: an older cover thread stops, the newer refresh queues what is missing.
static COVER_GEN: AtomicU64 = AtomicU64::new(0);

/// Tracks of one album share a cover; a track without an album has its own.
fn cover_key(t: &Track) -> String {
    match &t.album {
        Some(album) => format!(
            "{}\u{1f}{}",
            t.album_artist.as_deref().or(t.artist.as_deref()).unwrap_or(""),
            album
        ),
        None => t.path.to_string_lossy().into_owned(),
    }
}

/// Reads and shrinks the cover of one track (96 px is enough for the 40 px square on HiDPI screens).
fn decode_cover(path: &Path) -> Option<CoverPixels> {
    decode_cover_sized(path, 96)
}

fn decode_cover_sized(path: &Path, size: u32) -> Option<CoverPixels> {
    let bytes = radiant_core::library::read_cover(path)?;
    let thumb = image::load_from_memory(&bytes).ok()?.thumbnail(size, size).to_rgba8();
    let (width, height) = thumb.dimensions();
    Some(CoverPixels { width, height, rgba: Arc::new(thumb.into_raw()) })
}

fn load_covers(weak: slint::Weak<MainWindow>, jobs: Vec<(String, PathBuf)>, generation: u64) {
    if jobs.is_empty() {
        return;
    }
    std::thread::spawn(move || {
        for (key, path) in jobs {
            if COVER_GEN.load(Ordering::Acquire) != generation {
                return;
            }
            let pixels = decode_cover(&path);
            if let Ok(mut cache) = COVER_CACHE.lock() {
                if cache.len() >= 600 { cache.clear(); }
                cache.insert(key.clone(), pixels.clone());
            }
            let Some(px) = pixels else { continue };
            let _ = weak.upgrade_in_event_loop(move |ui| apply_cover(&ui, &key, &px));
        }
    });
}

/// Puts a freshly decoded cover into every row with the same key.
fn apply_cover(ui: &MainWindow, key: &str, px: &CoverPixels) {
    let image = px.to_image();
    let keys = ROW_KEYS.lock().map(|k| k.clone()).unwrap_or_default();
    let model = ui.get_tracks();
    for (i, k) in keys.iter().enumerate() {
        if k == key {
            if let Some(mut row) = model.row_data(i) {
                row.cover = image.clone();
                row.has_cover = true;
                model.set_row_data(i, row);
            }
        }
    }
    if NOW_KEY.lock().map(|k| *k == key).unwrap_or(false) {
        set_now_cover(ui, px);
    }
}

/// Cover key of the track shown in the big player view.
static NOW_KEY: Mutex<String> = Mutex::new(String::new());
/// Path of the track the big player view describes (to drop late results for an older track).
static LAST_NOW: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Start times (ms) of the synchronized lyrics of the current track; empty = no synced lyrics.
static LYRIC_TIMES: Mutex<Vec<u64>> = Mutex::new(Vec::new());
/// The current lyric line starts a little early so the eye can follow it.
const LEAD_MS: u64 = 250;

const ACCENT_RGB: (u8, u8, u8) = (0x8b, 0x6c, 0xff);

/// Glow color of the big player view: the cover's most colorful tone, lifted so it always shines.
fn dominant_color(px: &CoverPixels) -> slint::Color {
    let data = &px.rgba;
    let n = data.len() / 4;
    let step = (n / 400).max(1);
    let (mut r, mut g, mut b, mut weight_sum) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
    for i in (0..n).step_by(step) {
        let p = &data[i * 4..i * 4 + 4];
        if p[3] < 128 { continue; }
        let (rf, gf, bf) = (p[0] as f32, p[1] as f32, p[2] as f32);
        let max = rf.max(gf).max(bf);
        let min = rf.min(gf).min(bf);
        let saturation = if max > 0.0 { (max - min) / max } else { 0.0 };
        // Colorful, bright pixels count most; the small constant keeps gray covers working.
        let w = saturation * saturation * (max / 255.0) + 0.001;
        r += rf * w;
        g += gf * w;
        b += bf * w;
        weight_sum += w;
    }
    if weight_sum <= 0.0 {
        return slint::Color::from_rgb_u8(ACCENT_RGB.0, ACCENT_RGB.1, ACCENT_RGB.2);
    }
    let (mut r, mut g, mut b) = (r / weight_sum, g / weight_sum, b / weight_sum);
    let max = r.max(g).max(b);
    if max > 0.0 && max < 170.0 {
        let k = 170.0 / max;
        r *= k;
        g *= k;
        b *= k;
    }
    slint::Color::from_rgb_u8(r.min(255.0) as u8, g.min(255.0) as u8, b.min(255.0) as u8)
}

fn set_now_cover(ui: &MainWindow, px: &CoverPixels) {
    ui.set_np_cover(px.to_image());
    ui.set_np_has_cover(true);
    ui.set_glow_color(dominant_color(px));
}

fn clear_now_cover(ui: &MainWindow) {
    ui.set_np_cover(slint::Image::default());
    ui.set_np_has_cover(false);
    ui.set_glow_color(slint::Color::from_rgb_u8(ACCENT_RGB.0, ACCENT_RGB.1, ACCENT_RGB.2));
}

/// Decodes (or takes from the cache) the cover of the playing track on a helper thread.
fn request_cover(weak: slint::Weak<MainWindow>, key: String, path: PathBuf) {
    std::thread::spawn(move || {
        let cached = COVER_CACHE.lock().ok().and_then(|c| c.get(&key).cloned());
        let pixels = match cached {
            Some(p) => p,
            None => {
                let p = decode_cover(&path);
                if let Ok(mut cache) = COVER_CACHE.lock() {
                    if cache.len() >= 600 { cache.clear(); }
                    cache.insert(key.clone(), p.clone());
                }
                p
            }
        };
        let Some(px) = pixels else { return };
        let _ = weak.upgrade_in_event_loop(move |ui| apply_cover(&ui, &key, &px));
    });
}

/// "FLAC · 1411 kbps · 44.1 kHz"
fn quality_text(info: &AudioInfo) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !info.format.is_empty() { parts.push(info.format.clone()); }
    if let Some(kbps) = info.bitrate_kbps { parts.push(format!("{kbps} kbps")); }
    if let Some(rate) = info.sample_rate { parts.push(format!("{} kHz", rate as f32 / 1000.0)); }
    parts.join(" · ")
}

fn fmt_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1.0 { format!("{mb:.1} MB") } else { format!("{:.0} KB", bytes as f64 / 1024.0) }
}

fn info_rows(t: &Track) -> Vec<InfoRow> {
    let mut rows: Vec<(&str, String)> = Vec::new();
    // Tags first, then technical data, then the file itself.
    rows.push(("Title", t.title.clone()));
    rows.push(("Artist", t.artist.clone().unwrap_or_default()));
    rows.push(("Album artist", t.album_artist.clone().unwrap_or_default()));
    rows.push(("Album", t.album.clone().unwrap_or_default()));
    rows.push(("Track no.", t.track_no.filter(|n| *n > 0).map(|n| n.to_string()).unwrap_or_default()));
    rows.push(("Year", t.year.map(|y| y.to_string()).unwrap_or_default()));
    rows.push(("Format", t.info.format.clone()));
    rows.push(("Bitrate", t.info.bitrate_kbps.filter(|k| *k > 0).map(|k| format!("{k} kbps")).unwrap_or_default()));
    rows.push(("Sample rate", t.info.sample_rate.map(|r| format!("{} kHz", r as f32 / 1000.0)).unwrap_or_default()));
    rows.push(("Channels", match t.info.channels {
        Some(1) => "Mono".to_string(),
        Some(2) => "Stereo".to_string(),
        Some(n) if n > 0 => format!("{n} channels"),
        _ => String::new(),
    }));
    rows.push(("Duration", t.duration_ms.filter(|d| *d > 0).map(fmt_time).unwrap_or_default()));
    rows.push(("Size", if t.info.size_bytes > 0 { fmt_size(t.info.size_bytes) } else { String::new() }));
    rows.push(("Lyrics", if t.lyrics_path.is_some() { "File next to the track".to_string() }
        else if t.tag_lyrics { "Embedded in tags".to_string() } else { String::new() }));
    rows.push(("File", t.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    rows.push(("Folder", t.path.parent().map(|p| p.display().to_string()).unwrap_or_default()));
    rows.into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(label, value)| InfoRow { label: label.into(), value: value.into() })
        .collect()
}

/// Fills the big player view for a newly started track.
fn show_now_playing(ui: &MainWindow, track: &Track) {
    ui.set_np_title(track.title.clone().into());
    let sub: Vec<String> = [track.artist.clone(), track.album.clone(), track.year.map(|y| y.to_string())]
        .into_iter()
        .flatten()
        .collect();
    ui.set_np_sub(sub.join(" · ").into());
    ui.set_np_quality(quality_text(&track.info).into());
    ui.set_np_letter(track.title.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default().into());
    ui.set_np_info(ModelRc::new(VecModel::from(info_rows(track))));

    let key = cover_key(track);
    if let Ok(mut k) = NOW_KEY.lock() { *k = key.clone(); }
    let cached = COVER_CACHE.lock().ok().and_then(|c| c.get(&key).cloned());
    match cached {
        Some(Some(px)) => set_now_cover(ui, &px),
        Some(None) => clear_now_cover(ui),
        None => {
            clear_now_cover(ui);
            request_cover(ui.as_weak(), key, track.path.clone());
        }
    }

    // Lyrics are read on a helper thread; a late result for an older track is dropped.
    ui.set_lyrics(ModelRc::new(VecModel::from(Vec::<LyricRow>::new())));
    ui.set_lyrics_synced(false);
    ui.set_lyric_index(-1);
    if let Ok(mut times) = LYRIC_TIMES.lock() { times.clear(); }
    let weak = ui.as_weak();
    let owned = track.clone();
    std::thread::spawn(move || {
        let lines = radiant_core::lyrics::load(&owned);
        let _ = weak.upgrade_in_event_loop(move |ui| {
            if LAST_NOW.lock().ok().and_then(|l| l.clone()).as_ref() != Some(&owned.path) {
                return;
            }
            let synced = radiant_core::lyrics::is_synced(&lines);
            if let Ok(mut times) = LYRIC_TIMES.lock() {
                *times = if synced { lines.iter().filter_map(|l| l.time_ms).collect() } else { Vec::new() };
            }
            let rows: Vec<LyricRow> = lines
                .iter()
                .map(|l| LyricRow {
                    text: l.text.clone().into(),
                    time_ms: l.time_ms.map_or(-1, |t| t.min(i32::MAX as u64) as i32),
                })
                .collect();
            ui.set_lyrics(ModelRc::new(VecModel::from(rows)));
            ui.set_lyrics_synced(synced);
        });
    });
}

/// Track order chosen by hand (mirrored in `AppSettings::manual_order`, which is saved to disk).
static MANUAL_ORDER: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

fn scan_folder(
    weak: slint::Weak<MainWindow>,
    library: Arc<Mutex<Vec<Track>>>,
    displayed: Arc<Mutex<Vec<PathBuf>>>,
    query: Arc<Mutex<String>>,
    sort: Arc<Mutex<SortState>>,
    scan_gen: Arc<AtomicU64>,
    dir: PathBuf,
) {
    // A slower scan must not overwrite the list from a newer one.
    let ticket = scan_gen.fetch_add(1, Ordering::AcqRel) + 1;
    if let Some(ui) = weak.upgrade() {
        ui.set_status("Scanning...".into());
    }
    std::thread::spawn(move || {
        let tracks = radiant_core::library::scan(&dir);
        let weak_for_ui = weak.clone();
        let _ = weak.upgrade_in_event_loop(move |_ui| {
            if scan_gen.load(Ordering::Acquire) != ticket {
                return;
            }
            if let Ok(mut l) = library.lock() { *l = tracks; }
            refresh_tracks(&weak_for_ui, &library, &displayed, &query, *sort.lock().unwrap_or_else(|e| e.into_inner()));
        });
    });
}

fn sort_key_from_ui(mode: i32) -> SortKey {
    match mode {
        0 => SortKey::Manual,
        2 => SortKey::Artist,
        3 => SortKey::Album,
        4 => SortKey::Duration,
        5 => SortKey::FileDate,
        _ => SortKey::Name,
    }
}

fn refresh_tracks(
    weak: &slint::Weak<MainWindow>,
    library: &Arc<Mutex<Vec<Track>>>,
    displayed: &Arc<Mutex<Vec<PathBuf>>>,
    query: &Arc<Mutex<String>>,
    state: SortState,
) {
    let mut tracks = library.lock().map(|l| l.clone()).unwrap_or_default();
    if state.key == SortKey::Manual {
        let order = MANUAL_ORDER.lock().map(|o| o.clone()).unwrap_or_default();
        radiant_core::sort::apply_manual_order(&mut tracks, &order);
    } else {
        radiant_core::sort::sort_tracks(&mut tracks, state);
    }
    // With a search text the list is ordered by relevance (closer match = higher); equal scores keep
    // the order of the chosen sort mode, because the sort below is stable.
    let needle = query.lock().map(|q| q.trim().to_lowercase()).unwrap_or_default();
    let visible: Vec<&Track> = if needle.is_empty() {
        tracks.iter().collect()
    } else {
        let mut scored: Vec<(u32, &Track)> = tracks
            .iter()
            .filter_map(|t| radiant_core::sort::search_score(t, &needle).map(|score| (score, t)))
            .collect();
        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, t)| t).collect()
    };
    let paths: Vec<PathBuf> = visible.iter().map(|t| t.path.clone()).collect();

    // One row per visible track. Covers already in the cache are used at once; the rest are
    // decoded on a helper thread and filled in later (`load_covers`).
    let generation = COVER_GEN.fetch_add(1, Ordering::AcqRel) + 1;
    let mut rows: Vec<TrackRow> = Vec::with_capacity(visible.len());
    let mut keys: Vec<String> = Vec::with_capacity(visible.len());
    let mut jobs: Vec<(String, PathBuf)> = Vec::new();
    let mut queued: HashSet<String> = HashSet::new();
    let mut images: HashMap<String, slint::Image> = HashMap::new();
    for t in &visible {
        let key = cover_key(t);
        let cached = COVER_CACHE.lock().ok().and_then(|c| c.get(&key).cloned());
        let (cover, has_cover) = match cached {
            Some(Some(px)) => (images.entry(key.clone()).or_insert_with(|| px.to_image()).clone(), true),
            Some(None) => (slint::Image::default(), false),
            None => {
                if queued.insert(key.clone()) {
                    jobs.push((key.clone(), t.path.clone()));
                }
                (slint::Image::default(), false)
            }
        };
        rows.push(TrackRow {
            title: t.title.clone().into(),
            artist: t.artist.clone().unwrap_or_default().into(),
            album: t.album.clone().unwrap_or_default().into(),
            year: t.year.map(|y| y.to_string()).unwrap_or_default().into(),
            duration: t.duration_ms.filter(|d| *d > 0).map(fmt_time).unwrap_or_default().into(),
            letter: t.title.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default().into(),
            cover,
            has_cover,
        });
        keys.push(key);
    }
    if let Ok(mut row_keys) = ROW_KEYS.lock() { *row_keys = keys; }
    if let Some(ui) = weak.upgrade() {
        load_covers(ui.as_weak(), jobs, generation);
    }
    if let Ok(mut all) = library.lock() { *all = tracks; }
    if let Ok(mut shown) = displayed.lock() { *shown = paths; }
    if let Some(ui) = weak.upgrade() {
        ui.set_tracks(ModelRc::new(VecModel::from(rows)));
        ui.set_status("".into());
    }
    refresh_albums(weak, library, query);
}

// ---- Albums grid ----

/// Cover size of the album tiles. 224 px is about 200 KB per cover; covers are decoded only for tiles
/// that were on screen (the grid is virtualized and tells us through `album-shown`).
const ALBUM_COVER_PX: u32 = 224;
/// Decoded album covers kept in memory (the key is the album key).
const ALBUM_CACHE_MAX: usize = 240;

/// One album of the grid, in the order of the model.
struct AlbumEntry {
    key: String,
    /// Tracks of the album in play order (disc, track number, then title).
    paths: Vec<PathBuf>,
}

static ALBUMS: Mutex<Vec<AlbumEntry>> = Mutex::new(Vec::new());
/// Rows of the open album page (same order as the model): the file of a track, `None` for a "Disc N" caption.
static OPEN_ROWS: Mutex<Vec<Option<PathBuf>>> = Mutex::new(Vec::new());
static ALBUM_COVERS: Mutex<BTreeMap<String, Option<CoverPixels>>> = Mutex::new(BTreeMap::new());
/// Albums whose cover is being decoded right now (no duplicate work while a tile scrolls in and out).
static ALBUM_PENDING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Rebuilds the album grid from the library. The search box filters albums by name and artist.
fn refresh_albums(weak: &slint::Weak<MainWindow>, library: &Arc<Mutex<Vec<Track>>>, query: &Arc<Mutex<String>>) {
    let tracks = library.lock().map(|l| l.clone()).unwrap_or_default();
    let needle = query.lock().map(|q| q.trim().to_lowercase()).unwrap_or_default();
    let by_path: HashMap<&Path, &Track> = tracks.iter().map(|t| (t.path.as_path(), t)).collect();

    let mut albums = radiant_core::library::group_albums(&tracks);
    if !needle.is_empty() {
        albums.retain(|a| {
            a.name.as_deref().unwrap_or("").to_lowercase().contains(&needle)
                || a.artist.as_deref().unwrap_or("").to_lowercase().contains(&needle)
        });
    }
    // By name; albums without a name go last. Equal names: by artist.
    albums.sort_by_key(|a| (a.name.is_none(), a.name.clone().unwrap_or_default().to_lowercase(), a.artist.clone().unwrap_or_default().to_lowercase()));

    let cache = ALBUM_COVERS.lock().map(|c| c.clone()).unwrap_or_default();
    let mut entries: Vec<AlbumEntry> = Vec::with_capacity(albums.len());
    let mut rows: Vec<AlbumRow> = Vec::with_capacity(albums.len());
    for a in albums {
        let mut paths = a.tracks.clone();
        paths.sort_by_key(|p| {
            let t = by_path.get(p.as_path());
            (
                t.and_then(|t| t.disc_no).unwrap_or(1),
                t.and_then(|t| t.track_no).unwrap_or(u32::MAX),
                t.map(|t| t.title.to_lowercase()).unwrap_or_default(),
            )
        });
        let title = a.name.clone().unwrap_or_else(|| "Unknown album".to_string());
        let (cover, has_cover) = match cache.get(&a.key) {
            Some(Some(px)) => (px.to_image(), true),
            _ => (slint::Image::default(), false),
        };
        rows.push(AlbumRow {
            letter: title.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default().into(),
            title: title.into(),
            artist: a.artist.clone().unwrap_or_else(|| "Unknown artist".to_string()).into(),
            cover,
            has_cover,
        });
        entries.push(AlbumEntry { key: a.key, paths });
    }
    if let Ok(mut all) = ALBUMS.lock() { *all = entries; }
    if let Some(ui) = weak.upgrade() {
        ui.set_albums(ModelRc::new(VecModel::from(rows)));
    }
}

/// Total length: "54:12", or "1:02:30" from an hour up.
fn fmt_total(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s % 3600 / 60, s % 60)
    } else {
        fmt_time(ms)
    }
}

/// Fills the album page with the tracks of album `index` (tag order: disc, then track number).
/// Multi-disc albums get a "Disc N" caption row before each disc. The cover is the one of the tile.
fn open_album(ui: &MainWindow, library: &Arc<Mutex<Vec<Track>>>, index: usize) {
    let Some(paths) = ALBUMS.lock().ok().and_then(|a| a.get(index).map(|e| e.paths.clone())) else { return };
    let Some(tile) = ui.get_albums().row_data(index) else { return };
    let tracks = library.lock().map(|l| l.clone()).unwrap_or_default();
    let by_path: HashMap<&Path, &Track> = tracks.iter().map(|t| (t.path.as_path(), t)).collect();
    let album_artist = tile.artist.to_string();

    let discs: BTreeSet<u32> = paths.iter().map(|p| by_path.get(p.as_path()).and_then(|t| t.disc_no).unwrap_or(1)).collect();
    let multi_disc = discs.len() > 1;

    let mut rows: Vec<AlbumTrackRow> = Vec::with_capacity(paths.len() + discs.len());
    let mut row_paths: Vec<Option<PathBuf>> = Vec::with_capacity(paths.len() + discs.len());
    let mut last_disc: Option<u32> = None;
    let mut seq = 0_u32;
    let mut total_ms = 0_u64;
    for p in &paths {
        let t = by_path.get(p.as_path()).copied();
        let disc = t.and_then(|t| t.disc_no).unwrap_or(1);
        if multi_disc && last_disc != Some(disc) {
            rows.push(AlbumTrackRow { header: format!("Disc {disc}").into(), ..Default::default() });
            row_paths.push(None);
            last_disc = Some(disc);
            seq = 0;
        }
        seq += 1;
        let duration = t.and_then(|t| t.duration_ms).filter(|d| *d > 0);
        total_ms += duration.unwrap_or(0);
        // The number from the tag; a file without one gets its place in the order.
        let no = t.and_then(|t| t.track_no).filter(|n| *n > 0).unwrap_or(seq);
        let artist = t.and_then(|t| t.artist.clone()).filter(|a| *a != album_artist).unwrap_or_default();
        rows.push(AlbumTrackRow {
            no: no.to_string().into(),
            title: t.map(|t| t.title.clone()).unwrap_or_else(|| p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()).into(),
            artist: artist.into(),
            duration: duration.map(fmt_time).unwrap_or_default().into(),
            header: SharedString::default(),
        });
        row_paths.push(Some(p.clone()));
    }

    let count = paths.len();
    let mut meta = format!("{} {}", count, if count == 1 { "track" } else { "tracks" });
    if total_ms > 0 {
        meta.push_str(&format!(" · {}", fmt_total(total_ms)));
    }
    let playing = LAST_NOW.lock().ok().and_then(|l| l.clone());
    let current = playing
        .and_then(|c| row_paths.iter().position(|p| p.as_ref() == Some(&c)))
        .map_or(-1, |i| i as i32);

    if let Ok(mut open) = OPEN_ROWS.lock() { *open = row_paths; }
    ui.set_album_title(tile.title.clone());
    ui.set_album_artist(tile.artist.clone());
    ui.set_album_meta(meta.into());
    ui.set_album_cover(tile.cover.clone());
    ui.set_album_has_cover(tile.has_cover);
    ui.set_album_letter(tile.letter.clone());
    ui.set_album_current_index(current);
    ui.set_album_tracks(ModelRc::new(VecModel::from(rows)));
}

/// A tile came into view: make sure its cover is decoded and shown.
fn album_cover_wanted(weak: slint::Weak<MainWindow>, index: usize) {
    let Some((key, paths)) = ALBUMS.lock().ok().and_then(|a| a.get(index).map(|e| (e.key.clone(), e.paths.clone()))) else { return };
    let cached = ALBUM_COVERS.lock().ok().and_then(|c| c.get(&key).cloned());
    match cached {
        Some(Some(px)) => {
            let _ = weak.upgrade_in_event_loop(move |ui| apply_album_cover(&ui, &key, &px));
        }
        Some(None) => {} // looked for, none found
        None => {
            if !ALBUM_PENDING.lock().map(|mut p| p.insert(key.clone())).unwrap_or(false) {
                return;
            }
            std::thread::spawn(move || {
                // The first tracks of the album usually carry the cover; a few tries cover odd files.
                let pixels = paths.iter().take(4).find_map(|p| decode_cover_sized(p, ALBUM_COVER_PX));
                if let Ok(mut cache) = ALBUM_COVERS.lock() {
                    if cache.len() >= ALBUM_CACHE_MAX { cache.clear(); }
                    cache.insert(key.clone(), pixels.clone());
                }
                if let Ok(mut p) = ALBUM_PENDING.lock() { p.remove(&key); }
                if let Some(px) = pixels {
                    let _ = weak.upgrade_in_event_loop(move |ui| apply_album_cover(&ui, &key, &px));
                }
            });
        }
    }
}

/// Puts a decoded cover into the tile of its album (the position is looked up again: the grid may have changed).
fn apply_album_cover(ui: &MainWindow, key: &str, px: &CoverPixels) {
    let Some(i) = ALBUMS.lock().ok().and_then(|a| a.iter().position(|e| e.key == key)) else { return };
    let model = ui.get_albums();
    if let Some(mut row) = model.row_data(i) {
        row.cover = px.to_image();
        row.has_cover = true;
        // The album page of this album is open already (opened before its cover was decoded).
        if ui.get_album_open() && ui.get_album_title() == row.title && ui.get_album_artist() == row.artist {
            ui.set_album_cover(row.cover.clone());
            ui.set_album_has_cover(true);
        }
        model.set_row_data(i, row);
    }
}

// ---- Playlists ----

/// Everything the playlist windows need (UI thread only).
struct PlState {
    store: PlaylistStore,
    /// Playlist ids of the sleeves in the "Add to playlist" window (same order as `pl-cards`).
    card_ids: Vec<String>,
    /// What each line of the Playlists page is (same order as `pl-rows`).
    rows: Vec<ListEntry>,
    /// Track the window works on; `None` = opened to create a playlist or to move one.
    target: Option<PathBuf>,
    /// Playlist that the folder view is about to move.
    moving: Option<String>,
    /// Playlist whose page is open.
    open: Option<String>,
    /// What the rename / delete window is about.
    edit: Option<Edit>,
    /// Sort of the Playlists page (saved in `settings.json`).
    sort: PlaylistSort,
}

/// Target of the rename / delete windows (an id in the store).
enum Edit {
    Playlist(String),
    Folder(String),
}

/// Tracks of the open playlist page, same order as its rows (the file of every row).
static PL_ROWS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

type PlRef = Rc<RefCell<PlState>>;

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to(r), to(g), to(b))
}

/// Two colors of a playlist sleeve, taken from its id so they never change.
fn playlist_colors(id: &str) -> (slint::Color, slint::Color) {
    if id == FAVORITES_ID {
        return (slint::Color::from_rgb_u8(0xff, 0x5d, 0x8f), slint::Color::from_rgb_u8(0x8b, 0x6c, 0xff));
    }
    let mut h: u32 = 0x811c_9dc5;
    for b in id.bytes() {
        h ^= b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    let hue = (h % 360) as f32;
    let (r1, g1, b1) = hsl_to_rgb(hue, 0.62, 0.55);
    let (r2, g2, b2) = hsl_to_rgb(hue + 40.0, 0.58, 0.36);
    (slint::Color::from_rgb_u8(r1, g1, b1), slint::Color::from_rgb_u8(r2, g2, b2))
}

fn pl_name(p: &Playlist) -> String {
    if p.id == FAVORITES_ID { "Liked tracks".to_string() } else { p.name.clone() }
}

fn tracks_text(n: usize) -> String {
    format!("{n} {}", if n == 1 { "track" } else { "tracks" })
}

fn first_letter(text: &str) -> SharedString {
    text.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default().into()
}

fn make_card(st: &PlState, index: usize) -> PlaylistCard {
    let p = &st.store.playlists[index];
    let (color_a, color_b) = playlist_colors(&p.id);
    let name = pl_name(p);
    PlaylistCard {
        letter: first_letter(&name),
        name: name.into(),
        count: tracks_text(p.track_paths.len()).into(),
        color_a,
        color_b,
        added: st.target.as_ref().map_or(false, |t| p.track_paths.contains(t)),
        liked: p.id == FAVORITES_ID,
    }
}

/// Rebuilds the sleeves of the "Add to playlist" window, the folder list and the Playlists page.
fn refresh_playlists(ui: &MainWindow, st: &mut PlState) {
    let order = st.store.popup_order();
    st.card_ids = order.iter().map(|&i| st.store.playlists[i].id.clone()).collect();
    let cards: Vec<PlaylistCard> = order.iter().map(|&i| make_card(st, i)).collect();
    ui.set_pl_cards(ModelRc::new(VecModel::from(cards)));

    let folders: Vec<FolderChip> = st
        .store
        .folder_order()
        .into_iter()
        .map(|i| FolderChip { id: st.store.folders[i].id.clone().into(), name: st.store.folders[i].name.clone().into() })
        .collect();
    ui.set_pl_folders(ModelRc::new(VecModel::from(folders)));
    refresh_playlist_rows(ui, st);
}

fn refresh_playlist_rows(ui: &MainWindow, st: &mut PlState) {
    st.rows = st.store.list_entries_sorted(st.sort);
    let rows: Vec<PlaylistListRow> = st
        .rows
        .iter()
        .map(|entry| match *entry {
            ListEntry::Folder(fi) => {
                let f = &st.store.folders[fi];
                let n = st.store.playlists.iter().filter(|p| p.folder_id.as_deref() == Some(f.id.as_str())).count();
                PlaylistListRow {
                    is_folder: true,
                    name: f.name.clone().into(),
                    sub: format!("{n} {}", if n == 1 { "playlist" } else { "playlists" }).into(),
                    collapsed: f.collapsed,
                    ..Default::default()
                }
            }
            ListEntry::Playlist(pi) => {
                let p = &st.store.playlists[pi];
                let (color_a, color_b) = playlist_colors(&p.id);
                let name = pl_name(p);
                PlaylistListRow {
                    is_folder: false,
                    letter: first_letter(&name),
                    name: name.into(),
                    sub: tracks_text(p.track_paths.len()).into(),
                    color_a,
                    color_b,
                    liked: p.id == FAVORITES_ID,
                    collapsed: false,
                    indent: p.folder_id.is_some(),
                }
            }
        })
        .collect();
    ui.set_pl_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_pl_total(st.store.playlists.len() as i32);
}

fn save_playlists(ui: &MainWindow, store: &PlaylistStore) {
    if let Err(e) = playlists::save(store) {
        ui.set_status(format!("Could not save playlists: {e}").into());
    }
}

/// Fills the open playlist page. A playlist that no longer exists closes the page.
fn fill_playlist_page(ui: &MainWindow, st: &PlState, library: &Arc<Mutex<Vec<Track>>>) {
    let Some(id) = st.open.as_deref() else { return };
    let Some(p) = st.store.playlist(id) else {
        ui.set_pl_open(false);
        return;
    };
    let tracks = library.lock().map(|l| l.clone()).unwrap_or_default();
    let by_path: HashMap<&Path, &Track> = tracks.iter().map(|t| (t.path.as_path(), t)).collect();
    let mut rows: Vec<PlTrackRow> = Vec::with_capacity(p.track_paths.len());
    let mut total_ms = 0_u64;
    for (i, path) in p.track_paths.iter().enumerate() {
        let no = (i + 1).to_string().into();
        match by_path.get(path.as_path()) {
            Some(t) => {
                let duration = t.duration_ms.filter(|d| *d > 0);
                total_ms += duration.unwrap_or(0);
                rows.push(PlTrackRow {
                    no,
                    title: t.title.clone().into(),
                    artist: t.artist.clone().unwrap_or_default().into(),
                    duration: duration.map(fmt_time).unwrap_or_default().into(),
                    missing: false,
                });
            }
            None => rows.push(PlTrackRow {
                no,
                title: path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default().into(),
                artist: "File not found".into(),
                duration: SharedString::default(),
                missing: true,
            }),
        }
    }
    let mut meta = tracks_text(p.track_paths.len());
    if total_ms > 0 {
        meta.push_str(&format!(" · {}", fmt_total(total_ms)));
    }
    let name = pl_name(p);
    let (color_a, color_b) = playlist_colors(&p.id);
    let playing = LAST_NOW.lock().ok().and_then(|l| l.clone());
    let current = playing.and_then(|c| p.track_paths.iter().position(|t| *t == c)).map_or(-1, |i| i as i32);
    if let Ok(mut open) = PL_ROWS.lock() { *open = p.track_paths.clone(); }
    ui.set_pl_page_letter(first_letter(&name));
    ui.set_pl_page_title(name.into());
    ui.set_pl_page_meta(meta.into());
    ui.set_pl_page_liked(p.id == FAVORITES_ID);
    ui.set_pl_page_color_a(color_a);
    ui.set_pl_page_color_b(color_b);
    ui.set_pl_current_index(current);
    ui.set_pl_page_tracks(ModelRc::new(VecModel::from(rows)));
}

/// Tracks of the open playlist that are still in the library, in playlist order.
fn playable_queue(st: &PlState, library: &Arc<Mutex<Vec<Track>>>) -> Vec<PathBuf> {
    let Some(p) = st.open.as_deref().and_then(|id| st.store.playlist(id)) else { return Vec::new() };
    let known: HashSet<PathBuf> = library.lock().map(|l| l.iter().map(|t| t.path.clone()).collect()).unwrap_or_default();
    p.track_paths.iter().filter(|t| known.contains(*t)).cloned().collect()
}

/// Opens the "Add to playlist" window for one track (menu of the Tracks list, of an album or of a playlist page).
/// The playlists that already hold the track are ticked; clicking a ticked one takes the track out again.
fn open_add_window(ui: &MainWindow, pl: &PlRef, library: &Arc<Mutex<Vec<Track>>>, path: PathBuf) {
    let track = library.lock().ok().and_then(|l| l.iter().find(|t| t.path == path).cloned());
    let title = track
        .as_ref()
        .map(|t| t.title.clone())
        .unwrap_or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
    ui.set_pl_track_letter(first_letter(&title));
    ui.set_pl_track_title(title.into());
    ui.set_pl_track_artist(track.as_ref().and_then(|t| t.artist.clone()).unwrap_or_default().into());
    let cached = track
        .as_ref()
        .and_then(|t| COVER_CACHE.lock().ok().and_then(|c| c.get(&cover_key(t)).cloned()))
        .flatten();
    match cached {
        Some(px) => {
            ui.set_pl_track_cover(px.to_image());
            ui.set_pl_track_has_cover(true);
        }
        None => {
            ui.set_pl_track_cover(slint::Image::default());
            ui.set_pl_track_has_cover(false);
        }
    }
    let mut st = pl.borrow_mut();
    st.target = Some(path);
    st.moving = None;
    show_pl_popup(ui, &mut st, 0);
}

/// Shows the playlist window in `view` (0 = pick, 1 = create, 2 = move to folder).
fn show_pl_popup(ui: &MainWindow, st: &mut PlState, view: i32) {
    refresh_playlists(ui, st);
    ui.set_pl_has_track(st.target.is_some());
    ui.set_pl_name("".into());
    ui.set_pl_view(view);
    ui.set_show_pl_popup(true);
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;

    // Paths of the tracks shown in the list, in the same order (shared with the scan thread).
    let library: Arc<Mutex<Vec<Track>>> = Arc::default();
    let displayed: Arc<Mutex<Vec<PathBuf>>> = Arc::default();
    let query: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let sort: Arc<Mutex<SortState>> = Arc::new(Mutex::new(SortState { key: SortKey::Name, descending: false }));

    // The engine reports changes from its own thread; forward them to the UI thread.
    let weak = ui.as_weak();
    let status_library = displayed.clone();
    let now_library = library.clone();
    let player = Rc::new(Player::spawn(move |status| {
        // Position of the playing track in the list (-1 = none), for the row highlight.
        let index = status
            .current
            .as_ref()
            .and_then(|c| status_library.lock().ok()?.iter().position(|p| p == c))
            .map_or(-1, |i| i as i32);
        // The same for the open album page.
        let album_index = status
            .current
            .as_ref()
            .and_then(|c| OPEN_ROWS.lock().ok()?.iter().position(|p| p.as_ref() == Some(c)))
            .map_or(-1, |i| i as i32);

        // The same for the open playlist page.
        let pl_index = status
            .current
            .as_ref()
            .and_then(|c| PL_ROWS.lock().ok()?.iter().position(|p| p == c))
            .map_or(-1, |i| i as i32);

        // A new track started: the big player view needs its details (looked up only on a change).
        let changed_track: Option<Track> = match status.current.as_ref() {
            Some(current) => {
                let changed = LAST_NOW.lock().map_or(false, |mut last| {
                    if last.as_ref() != Some(current) {
                        *last = Some(current.clone());
                        true
                    } else {
                        false
                    }
                });
                if changed {
                    now_library.lock().ok().and_then(|l| l.iter().find(|t| &t.path == current).cloned())
                } else {
                    None
                }
            }
            None => None,
        };
        // Current line of the synchronized lyrics (binary search over the start times).
        let lyric_index = LYRIC_TIMES
            .lock()
            .ok()
            .filter(|times| !times.is_empty())
            .map_or(-1, |times| times.partition_point(|t| *t <= status.position_ms + LEAD_MS) as i32 - 1);

        let _ = weak.upgrade_in_event_loop(move |ui| {
            if let Some(track) = &changed_track {
                show_now_playing(&ui, track);
            }
            ui.set_lyric_index(lyric_index);
            ui.set_current_index(index);
            ui.set_album_current_index(album_index);
            ui.set_pl_current_index(pl_index);
            ui.set_shuffle(status.shuffle);
            ui.set_repeat_mode(match status.repeat {
                RepeatMode::Off => 0,
                RepeatMode::All => 1,
                RepeatMode::One => 2,
            });
            ui.set_position_text(fmt_time(status.position_ms).into());
            match status.duration_ms {
                Some(d) if d > 0 => {
                    ui.set_duration_text(fmt_time(d).into());
                    ui.set_duration_secs(d as f32 / 1000.0);
                    ui.set_progress((status.position_ms as f32 / d as f32).clamp(0.0, 1.0));
                }
                _ => {
                    ui.set_duration_text("".into());
                    ui.set_duration_secs(0.0);
                    ui.set_progress(0.0);
                }
            }
            let title = status
                .current
                .as_deref()
                .and_then(|p| p.file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            ui.set_now_playing(title.into());
            ui.set_is_playing(status.state == PlayState::Playing);
            if let Some(err) = status.error {
                ui.set_status(err.into());
            }
        });
    }));

    let settings = match settings::load() {
        Ok(s) => Rc::new(RefCell::new(s)),
        Err(e) => {
            ui.set_status(format!("Could not read settings: {e}").into());
            Rc::new(RefCell::new(AppSettings::default()))
        }
    };
    let pl: PlRef = {
        let store = match playlists::load() {
            Ok(store) => store,
            Err(e) => {
                ui.set_status(format!("Could not read playlists: {e}").into());
                let mut store = PlaylistStore::default();
                store.normalize();
                store
            }
        };
        let sort = PlaylistSort {
            key: PlaylistSortKey::from_index(settings.borrow().playlist_sort),
            descending: settings.borrow().playlist_sort_desc,
        };
        Rc::new(RefCell::new(PlState { store, card_ids: Vec::new(), rows: Vec::new(), target: None, moving: None, open: None, edit: None, sort }))
    };
    refresh_playlists(&ui, &mut pl.borrow_mut());
    let ui_weak = ui.as_weak();
    // Bumped on every folder scan so a late result cannot replace a newer one.
    let scan_gen: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

    // ---- sound settings (panel in the UI -> engine) ----
    // Rust owns the current settings; every change sends the whole set to the engine.
    let fx = Rc::new(RefCell::new(AudioFxSettings::default()));
    let eq_model: Rc<VecModel<f32>> = Rc::new(VecModel::from(vec![0.0_f32; 12]));
    ui.set_eq_gains(ModelRc::from(eq_model.clone()));
    apply_settings(&ui, &player, &fx, &eq_model, &settings.borrow());
    if let Ok(mut order) = MANUAL_ORDER.lock() { *order = settings.borrow().manual_order.clone(); }
    if let Some(dir) = settings.borrow().music_folder.clone() {
        scan_folder(ui_weak.clone(), library.clone(), displayed.clone(), query.clone(), sort.clone(), scan_gen.clone(), dir);
    }

    {
        let (weak, library, displayed, query, sort) = (ui_weak.clone(), library.clone(), displayed.clone(), query.clone(), sort.clone());
        ui.on_search_changed(move |value| {
            if let Ok(mut q) = query.lock() { *q = value.to_string(); }
            if let Some(ui) = weak.upgrade() { ui.set_album_open(false); }
            let state = *sort.lock().unwrap_or_else(|e| e.into_inner());
            refresh_tracks(&weak, &library, &displayed, &query, state);
        });
    }
    {
        let (weak, library, displayed, query, sort) = (ui_weak.clone(), library.clone(), displayed.clone(), query.clone(), sort.clone());
        ui.on_sort_changed(move |mode, descending| {
            let state = SortState { key: sort_key_from_ui(mode), descending };
            if let Ok(mut current) = sort.lock() { *current = state; }
            refresh_tracks(&weak, &library, &displayed, &query, state);
        });
    }

    {
        let (weak, library, displayed, query, sort, settings) = (ui_weak.clone(), library.clone(), displayed.clone(), query.clone(), sort.clone(), settings.clone());
        ui.on_reorder_tracks(move |from, to| {
            if from < 0 || to < 0 || query.lock().map(|q| !q.is_empty()).unwrap_or(true) {
                return;
            }
            // The list on screen (no search active) becomes the new manual order, with the dragged row moved.
            let mut order: Vec<PathBuf> = displayed.lock().map(|d| d.clone()).unwrap_or_default();
            let (from, to) = (from as usize, to as usize);
            if from >= order.len() || to >= order.len() || from == to { return; }
            let moved = order.remove(from);
            order.insert(to, moved);
            if let Ok(mut m) = MANUAL_ORDER.lock() { *m = order.clone(); }
            settings.borrow_mut().manual_order = order;
            save_settings(&weak, &settings);
            let state = SortState { key: SortKey::Manual, descending: false };
            if let Ok(mut current) = sort.lock() { *current = state; }
            if let Some(ui) = weak.upgrade() {
                ui.set_sort_mode(0);
                ui.set_sort_descending(false);
            }
            refresh_tracks(&weak, &library, &displayed, &query, state);
        });
    }

    {
        let player = player.clone();
        let library = library.clone();
        let displayed = displayed.clone();
        ui.on_play_track(move |i| {
            if i < 0 {
                return;
            }
            let Some(path) = displayed.lock().ok().and_then(|l| l.get(i as usize).cloned()) else { return; };
            let queue = library.lock().map(|l| l.iter().map(|t| t.path.clone()).collect::<Vec<_>>()).unwrap_or_default();
            let Some(start) = queue.iter().position(|p| p == &path) else { return; };
            player.send(Command::SetQueue { queue, start });
        });
    }
    {
        // Click on an album tile: open its page (the window slides it in itself).
        let (weak, library) = (ui_weak.clone(), library.clone());
        ui.on_open_album(move |i| {
            if i < 0 { return; }
            if let Some(ui) = weak.upgrade() {
                open_album(&ui, &library, i as usize);
            }
        });
    }
    {
        // Context menu "Open album": which album (index in the grid) holds the track at this row of the list.
        let (weak, displayed) = (ui_weak.clone(), displayed.clone());
        ui.on_album_index_of_track(move |i| {
            let Some(path) = displayed.lock().ok().and_then(|l| l.get(i.max(0) as usize).cloned()) else { return -1 };
            let found = ALBUMS.lock().ok().and_then(|a| a.iter().position(|e| e.paths.contains(&path)));
            match found {
                Some(index) => {
                    // The tile may never have been on screen: get its cover for the album page too.
                    album_cover_wanted(weak.clone(), index);
                    index as i32
                }
                None => -1,
            }
        });
    }
    {
        // Context menu stub: "Create stems" is not implemented yet.
        ui.on_create_stems(|_row, _in_album| { /* TODO: stems */ });
    }
    {
        // Sort panel of the Playlists page: the order is applied at once and remembered.
        let (weak, pl, settings) = (ui_weak.clone(), pl.clone(), settings.clone());
        ui.on_pl_sort_changed(move |mode, descending| {
            let Some(ui) = weak.upgrade() else { return };
            let mode = mode.clamp(0, 3) as u8;
            {
                let mut st = pl.borrow_mut();
                st.sort = PlaylistSort { key: PlaylistSortKey::from_index(mode), descending };
                refresh_playlist_rows(&ui, &mut st);
            }
            {
                let mut cfg = settings.borrow_mut();
                cfg.playlist_sort = mode;
                cfg.playlist_sort_desc = descending;
            }
            save_settings(&weak, &settings);
        });
    }
    {
        // "Add to playlist" button of the player view: works on the track that is playing now.
        let (weak, pl, library) = (ui_weak.clone(), pl.clone(), library.clone());
        ui.on_add_current_to_playlist(move || {
            let path = LAST_NOW.lock().ok().and_then(|l| l.clone());
            let (Some(path), Some(ui)) = (path, weak.upgrade()) else { return };
            open_add_window(&ui, &pl, &library, path);
        });
    }
    {
        // Context menu "Add to playlist": opens the window for the track of this row.
        let (weak, pl, library, displayed) = (ui_weak.clone(), pl.clone(), library.clone(), displayed.clone());
        ui.on_add_to_playlist(move |row, in_album| {
            let row = row.max(0) as usize;
            let path = if in_album {
                OPEN_ROWS.lock().ok().and_then(|r| r.get(row).cloned()).flatten()
            } else {
                displayed.lock().ok().and_then(|l| l.get(row).cloned())
            };
            let (Some(path), Some(ui)) = (path, weak.upgrade()) else { return };
            open_add_window(&ui, &pl, &library, path);
        });
    }
    {
        // The same menu entry on a track of the open playlist page: its playlist shows up ticked,
        // so a second click on it takes the track out of the playlist.
        let (weak, pl, library) = (ui_weak.clone(), pl.clone(), library.clone());
        ui.on_pl_page_add_to_playlist(move |row| {
            let path = PL_ROWS.lock().ok().and_then(|r| r.get(row.max(0) as usize).cloned());
            let (Some(path), Some(ui)) = (path, weak.upgrade()) else { return };
            open_add_window(&ui, &pl, &library, path);
        });
    }
    {
        // Menu "Open album" on a track of the playlist page.
        let weak = ui_weak.clone();
        ui.on_album_index_of_pl_track(move |i| {
            let Some(path) = PL_ROWS.lock().ok().and_then(|r| r.get(i.max(0) as usize).cloned()) else { return -1 };
            let found = ALBUMS.lock().ok().and_then(|a| a.iter().position(|e| e.paths.contains(&path)));
            match found {
                Some(index) => {
                    album_cover_wanted(weak.clone(), index);
                    index as i32
                }
                None => -1,
            }
        });
    }
    {
        // A sleeve was clicked: add the track to that playlist, or take it out again.
        let (weak, pl, library) = (ui_weak.clone(), pl.clone(), library.clone());
        ui.on_pl_toggle(move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = pl.borrow_mut();
            let i = i.max(0) as usize;
            let (Some(target), Some(id)) = (st.target.clone(), st.card_ids.get(i).cloned()) else { return };
            let changed = if st.store.contains(&id, &target) {
                st.store.remove_track(&id, &target)
            } else {
                st.store.add_track(&id, &target)
            };
            if !changed { return; }
            save_playlists(&ui, &st.store);
            // Only this sleeve changes, so its record slides instead of the whole grid being rebuilt.
            if let Some(index) = st.store.playlists.iter().position(|p| p.id == id) {
                let card = make_card(&st, index);
                ui.get_pl_cards().set_row_data(i, card);
            }
            refresh_playlist_rows(&ui, &mut st);
            fill_playlist_page(&ui, &st, &library);
        });
    }
    {
        // "Create": new playlist (with the track when the window was opened for one).
        let (weak, pl, library) = (ui_weak.clone(), pl.clone(), library.clone());
        ui.on_pl_create(move |name, folder| {
            let Some(ui) = weak.upgrade() else { return false };
            let mut st = pl.borrow_mut();
            let folder = folder.to_string();
            let folder = if folder.is_empty() { None } else { Some(folder) };
            let Some(id) = st.store.create_playlist(&name, folder.as_deref()) else { return false };
            if let Some(target) = st.target.clone() {
                st.store.add_track(&id, &target);
            }
            save_playlists(&ui, &st.store);
            refresh_playlists(&ui, &mut st);
            fill_playlist_page(&ui, &st, &library);
            true
        });
    }
    {
        // New folder made inside the window; its id is handed back so the picker selects it.
        let (weak, pl) = (ui_weak.clone(), pl.clone());
        ui.on_pl_folder_create(move |name| {
            let Some(ui) = weak.upgrade() else { return SharedString::default() };
            let mut st = pl.borrow_mut();
            let Some(id) = st.store.create_folder(&name) else { return SharedString::default() };
            save_playlists(&ui, &st.store);
            refresh_playlists(&ui, &mut st);
            id.into()
        });
    }
    {
        // "New playlist" button of the Playlists page.
        let (weak, pl) = (ui_weak.clone(), pl.clone());
        ui.on_pl_open_create(move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = pl.borrow_mut();
            st.target = None;
            st.moving = None;
            ui.set_pl_folder_sel("".into());
            show_pl_popup(&ui, &mut st, 1);
        });
    }
    {
        // Folder button of a playlist row: choose its folder.
        let (weak, pl) = (ui_weak.clone(), pl.clone());
        ui.on_pl_open_move(move |row| {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = pl.borrow_mut();
            let Some(ListEntry::Playlist(pi)) = st.rows.get(row.max(0) as usize).copied() else { return };
            let p = &st.store.playlists[pi];
            if p.id == FAVORITES_ID { return; }
            let (id, name, folder) = (p.id.clone(), p.name.clone(), p.folder_id.clone().unwrap_or_default());
            st.target = None;
            st.moving = Some(id);
            ui.set_pl_move_name(name.into());
            ui.set_pl_folder_sel(folder.into());
            show_pl_popup(&ui, &mut st, 2);
        });
    }
    {
        let (weak, pl) = (ui_weak.clone(), pl.clone());
        ui.on_pl_move(move |folder| {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = pl.borrow_mut();
            let Some(id) = st.moving.take() else { return };
            let folder = folder.to_string();
            let folder = if folder.is_empty() { None } else { Some(folder) };
            if st.store.set_playlist_folder(&id, folder.as_deref()) {
                save_playlists(&ui, &st.store);
                refresh_playlist_rows(&ui, &mut st);
            }
        });
    }
    {
        // Click on a line of the Playlists page: a folder folds / unfolds, a playlist opens its page.
        let (weak, pl, library) = (ui_weak.clone(), pl.clone(), library.clone());
        ui.on_pl_row_clicked(move |row| {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = pl.borrow_mut();
            match st.rows.get(row.max(0) as usize).copied() {
                Some(ListEntry::Folder(fi)) => {
                    let id = st.store.folders[fi].id.clone();
                    if st.store.toggle_folder_collapsed(&id) {
                        save_playlists(&ui, &st.store);
                        refresh_playlist_rows(&ui, &mut st);
                    }
                }
                Some(ListEntry::Playlist(pi)) => {
                    st.open = Some(st.store.playlists[pi].id.clone());
                    fill_playlist_page(&ui, &st, &library);
                    ui.set_pl_open(true);
                }
                None => {}
            }
        });
    }
    {
        // Click on a track of the open playlist: the queue is the playlist, starting at that track.
        let (weak, pl, player, library) = (ui_weak.clone(), pl.clone(), player.clone(), library.clone());
        ui.on_pl_play_track(move |i| {
            let Some(clicked) = PL_ROWS.lock().ok().and_then(|r| r.get(i.max(0) as usize).cloned()) else { return };
            let queue = playable_queue(&pl.borrow(), &library);
            match queue.iter().position(|p| *p == clicked) {
                Some(start) => player.send(Command::SetQueue { queue, start }),
                None => {
                    if let Some(ui) = weak.upgrade() { ui.set_status("This file is not in the library".into()); }
                }
            }
        });
    }
    {
        let (weak, pl, player, library) = (ui_weak.clone(), pl.clone(), player.clone(), library.clone());
        ui.on_pl_play_all(move || {
            let queue = playable_queue(&pl.borrow(), &library);
            if queue.is_empty() {
                if let Some(ui) = weak.upgrade() { ui.set_status("This playlist has no playable tracks yet".into()); }
                return;
            }
            player.send(Command::SetQueue { queue, start: 0 });
        });
    }
    {
        // "Shuffle" on the playlist page: shuffle on, start from a random track.
        let (weak, pl, player, library, settings) = (ui_weak.clone(), pl.clone(), player.clone(), library.clone(), settings.clone());
        ui.on_pl_shuffle_all(move || {
            let queue = playable_queue(&pl.borrow(), &library);
            if queue.is_empty() {
                if let Some(ui) = weak.upgrade() { ui.set_status("This playlist has no playable tracks yet".into()); }
                return;
            }
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as usize)
                .unwrap_or(0);
            let start = nanos % queue.len();
            player.send(Command::SetShuffle(true));
            settings.borrow_mut().shuffle = true;
            if let Some(ui) = weak.upgrade() { ui.set_shuffle(true); }
            save_settings(&weak, &settings);
            player.send(Command::SetQueue { queue, start });
        });
    }
    {
        // Drag on a row of the open playlist: the new order is saved with the playlist.
        let (weak, pl, library) = (ui_weak.clone(), pl.clone(), library.clone());
        ui.on_pl_move_track(move |from, to| {
            let Some(ui) = weak.upgrade() else { return };
            if from < 0 || to < 0 { return; }
            let mut st = pl.borrow_mut();
            let Some(id) = st.open.clone() else { return };
            if st.store.move_track(&id, from as usize, to as usize) {
                save_playlists(&ui, &st.store);
                fill_playlist_page(&ui, &st, &library);
            }
        });
    }
    {
        // "Rename" on the playlist page.
        let (weak, pl) = (ui_weak.clone(), pl.clone());
        ui.on_pl_open_rename_playlist(move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = pl.borrow_mut();
            let Some(p) = st.open.as_deref().and_then(|id| st.store.playlist(id)) else { return };
            if p.id == FAVORITES_ID { return; }
            let (id, name) = (p.id.clone(), p.name.clone());
            st.target = None;
            st.moving = None;
            st.edit = Some(Edit::Playlist(id));
            ui.set_pl_dialog_title("Rename playlist".into());
            show_pl_popup(&ui, &mut st, 3);
            ui.set_pl_name(name.into());
        });
    }
    {
        // "Delete" on the playlist page: asks first.
        let (weak, pl) = (ui_weak.clone(), pl.clone());
        ui.on_pl_open_delete_playlist(move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = pl.borrow_mut();
            let Some(p) = st.open.as_deref().and_then(|id| st.store.playlist(id)) else { return };
            if p.id == FAVORITES_ID { return; }
            let (id, name) = (p.id.clone(), p.name.clone());
            st.target = None;
            st.moving = None;
            st.edit = Some(Edit::Playlist(id));
            ui.set_pl_dialog_title("Delete playlist?".into());
            ui.set_pl_dialog_text(format!("\"{name}\" will be deleted. The tracks stay in your library.").into());
            show_pl_popup(&ui, &mut st, 4);
        });
    }
    {
        // Pencil on a folder header.
        let (weak, pl) = (ui_weak.clone(), pl.clone());
        ui.on_pl_open_rename_folder(move |row| {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = pl.borrow_mut();
            let Some(ListEntry::Folder(fi)) = st.rows.get(row.max(0) as usize).copied() else { return };
            let (id, name) = (st.store.folders[fi].id.clone(), st.store.folders[fi].name.clone());
            st.target = None;
            st.moving = None;
            st.edit = Some(Edit::Folder(id));
            ui.set_pl_dialog_title("Rename folder".into());
            show_pl_popup(&ui, &mut st, 3);
            ui.set_pl_name(name.into());
        });
    }
    {
        // Trash on a folder header: asks first; the playlists inside stay.
        let (weak, pl) = (ui_weak.clone(), pl.clone());
        ui.on_pl_open_delete_folder(move |row| {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = pl.borrow_mut();
            let Some(ListEntry::Folder(fi)) = st.rows.get(row.max(0) as usize).copied() else { return };
            let (id, name) = (st.store.folders[fi].id.clone(), st.store.folders[fi].name.clone());
            st.target = None;
            st.moving = None;
            st.edit = Some(Edit::Folder(id));
            ui.set_pl_dialog_title("Delete folder?".into());
            ui.set_pl_dialog_text(format!("The folder \"{name}\" will be deleted. Its playlists are kept and move out of the folder.").into());
            show_pl_popup(&ui, &mut st, 4);
        });
    }
    {
        // "Save" in the rename window.
        let (weak, pl, library) = (ui_weak.clone(), pl.clone(), library.clone());
        ui.on_pl_rename(move |name| {
            let Some(ui) = weak.upgrade() else { return false };
            let mut st = pl.borrow_mut();
            let ok = match st.edit.take() {
                Some(Edit::Playlist(id)) => st.store.rename_playlist(&id, &name),
                Some(Edit::Folder(id)) => st.store.rename_folder(&id, &name),
                None => false,
            };
            if ok {
                save_playlists(&ui, &st.store);
                refresh_playlists(&ui, &mut st);
                fill_playlist_page(&ui, &st, &library);
            }
            ok
        });
    }
    {
        // "Delete" in the confirm window.
        let (weak, pl) = (ui_weak.clone(), pl.clone());
        ui.on_pl_delete(move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = pl.borrow_mut();
            let ok = match st.edit.take() {
                Some(Edit::Playlist(id)) => {
                    if st.open.as_deref() == Some(id.as_str()) {
                        st.open = None;
                        ui.set_pl_open(false);
                    }
                    st.store.delete_playlist(&id)
                }
                Some(Edit::Folder(id)) => st.store.delete_folder(&id),
                None => false,
            };
            if ok {
                save_playlists(&ui, &st.store);
                refresh_playlists(&ui, &mut st);
            }
        });
    }
    {
        // Click on a track of the open album: the queue is the album, starting at that track.
        let player = player.clone();
        ui.on_play_album_track(move |i| {
            let Some(Some(clicked)) = OPEN_ROWS.lock().ok().and_then(|r| r.get(i.max(0) as usize).cloned()) else { return };
            let queue: Vec<PathBuf> = OPEN_ROWS.lock().map(|r| r.iter().flatten().cloned().collect()).unwrap_or_default();
            let Some(start) = queue.iter().position(|p| p == &clicked) else { return };
            player.send(Command::SetQueue { queue, start });
        });
    }
    {
        // "Play" on the album page: the whole album from its first track.
        let player = player.clone();
        ui.on_play_album(move || {
            let queue: Vec<PathBuf> = OPEN_ROWS.lock().map(|r| r.iter().flatten().cloned().collect()).unwrap_or_default();
            if queue.is_empty() { return; }
            player.send(Command::SetQueue { queue, start: 0 });
        });
    }
    {
        // "Shuffle" on the album page: turns shuffle on and starts from a random track of the album.
        let (player, settings, weak) = (player.clone(), settings.clone(), ui_weak.clone());
        ui.on_shuffle_album(move || {
            let queue: Vec<PathBuf> = OPEN_ROWS.lock().map(|r| r.iter().flatten().cloned().collect()).unwrap_or_default();
            if queue.is_empty() { return; }
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as usize)
                .unwrap_or(0);
            let start = nanos % queue.len();
            player.send(Command::SetShuffle(true));
            settings.borrow_mut().shuffle = true;
            if let Some(ui) = weak.upgrade() { ui.set_shuffle(true); }
            save_settings(&weak, &settings);
            player.send(Command::SetQueue { queue, start });
        });
    }
    {
        let weak = ui_weak.clone();
        ui.on_album_shown(move |i| {
            if i >= 0 {
                album_cover_wanted(weak.clone(), i as usize);
            }
        });
    }
    {
        let player = player.clone();
        ui.on_toggle_play(move || player.send(Command::Toggle));
    }
    {
        let (player, settings, weak) = (player.clone(), settings.clone(), ui_weak.clone());
        ui.on_volume_changed(move |v| {
            player.send(Command::SetVolume(v));
            settings.borrow_mut().volume = v;
            save_settings(&weak, &settings);
        });
    }
    {
        // Section / queue switch: remembered between launches.
        let (settings, weak) = (settings.clone(), ui_weak.clone());
        ui.on_section_changed(move |section, queue| {
            {
                let mut st = settings.borrow_mut();
                st.section = section.clamp(0, 3) as u8;
                st.queue_index = queue.clamp(0, 3) as u8;
            }
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, settings, weak) = (player.clone(), settings.clone(), ui_weak.clone());
        ui.on_shuffle_changed(move |on| {
            player.send(Command::SetShuffle(on));
            settings.borrow_mut().shuffle = on;
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, settings, weak) = (player.clone(), settings.clone(), ui_weak.clone());
        ui.on_repeat_changed(move |mode| {
            let repeat = repeat_mode_from_ui(mode);
            player.send(Command::SetRepeat(repeat_mode_to_engine(repeat)));
            settings.borrow_mut().repeat_mode = repeat;
            save_settings(&weak, &settings);
        });
    }
    {
        let player = player.clone();
        ui.on_seek(move |ratio| player.send(Command::SeekRatio(ratio)));
    }
    {
        let player = player.clone();
        ui.on_next_track(move || player.send(Command::Next));
    }
    {
        let player = player.clone();
        ui.on_previous_track(move || player.send(Command::Previous));
    }

    // ---- playback speed (phase vocoder in the engine, pitch preserved) ----
    {
        let (player, settings, weak) = (player.clone(), settings.clone(), ui_weak.clone());
        ui.on_speed_changed(move |t| {
            let t = (t * 10.0).round() / 10.0;
            // Slider position -2..=2 -> speed 1/3..3 (same mapping as Android).
            player.send(Command::SetSpeed(slider_to_speed(t)));
            settings.borrow_mut().speed_t = t;
            save_settings(&weak, &settings);
        });
    }

    {
        let (player, fx, settings, weak) = (player.clone(), fx.clone(), settings.clone(), ui_weak.clone());
        ui.on_normalize_changed(move |on| {
            let mut s = fx.borrow_mut();
            s.normalize = on;
            player.send(Command::SetFx(s.clone()));
            settings.borrow_mut().fx = s.clone();
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, fx, settings, weak) = (player.clone(), fx.clone(), settings.clone(), ui_weak.clone());
        ui.on_mono_changed(move |on| {
            let mut s = fx.borrow_mut();
            s.mono = on;
            player.send(Command::SetFx(s.clone()));
            settings.borrow_mut().fx = s.clone();
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, fx, settings, weak) = (player.clone(), fx.clone(), settings.clone(), ui_weak.clone());
        ui.on_bass_changed(move |v| {
            let mut s = fx.borrow_mut();
            s.bass_db = (v * 2.0).round() / 2.0; // 0.5 dB steps
            player.send(Command::SetFx(s.clone()));
            settings.borrow_mut().fx = s.clone();
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, fx, settings, weak) = (player.clone(), fx.clone(), settings.clone(), ui_weak.clone());
        ui.on_treble_changed(move |v| {
            let mut s = fx.borrow_mut();
            s.treble_db = (v * 2.0).round() / 2.0;
            player.send(Command::SetFx(s.clone()));
            settings.borrow_mut().fx = s.clone();
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, fx, settings, weak) = (player.clone(), fx.clone(), settings.clone(), ui_weak.clone());
        ui.on_subsonic_changed(move |v| {
            let mut s = fx.borrow_mut();
            s.subsonic_percent = v.round(); // whole percent
            player.send(Command::SetFx(s.clone()));
            settings.borrow_mut().fx = s.clone();
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, fx, eq_model, settings, weak) = (player.clone(), fx.clone(), eq_model.clone(), settings.clone(), ui_weak.clone());
        ui.on_eq_changed(move |i, v| {
            let i = i as usize;
            if i >= 12 {
                return;
            }
            let v = (v * 2.0).round() / 2.0;
            eq_model.set_row_data(i, v); // moves the slider
            let mut s = fx.borrow_mut();
            s.eq_gains_db[i] = v;
            player.send(Command::SetFx(s.clone()));
            settings.borrow_mut().fx = s.clone();
            save_settings(&weak, &settings);
        });
    }
    // Preset buttons: names for the UI, and applying a curve replaces all 12 gains at once.
    let preset_names: Vec<SharedString> = EQ_PRESETS.iter().map(|p| p.name.into()).collect();
    ui.set_eq_preset_names(ModelRc::new(VecModel::from(preset_names)));
    {
        let (player, fx, eq_model, settings, weak) = (player.clone(), fx.clone(), eq_model.clone(), settings.clone(), ui_weak.clone());
        ui.on_eq_preset_selected(move |i| {
            let Some(preset) = EQ_PRESETS.get(i as usize) else { return };
            for (band, g) in preset.gains.iter().enumerate() {
                eq_model.set_row_data(band, *g);
            }
            let mut s = fx.borrow_mut();
            s.eq_gains_db = preset.gains;
            player.send(Command::SetFx(s.clone()));
            settings.borrow_mut().fx = s.clone();
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, fx, eq_model, settings) = (player.clone(), fx.clone(), eq_model.clone(), settings.clone());
        let weak = ui.as_weak();
        ui.on_fx_reset(move || {
            let mut s = fx.borrow_mut();
            *s = AudioFxSettings::default();
            for i in 0..12 {
                eq_model.set_row_data(i, 0.0);
            }
            if let Some(ui) = weak.upgrade() {
                ui.set_fx_normalize(false);
                ui.set_fx_mono(false);
                ui.set_fx_bass(0.0);
                ui.set_fx_treble(0.0);
                ui.set_fx_subsonic(0.0);
            }
            player.send(Command::SetFx(s.clone()));
            settings.borrow_mut().fx = s.clone();
            save_settings(&weak, &settings);
        });
    }

    let weak = ui_weak.clone();
    ui.on_choose_folder(move || {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
        settings.borrow_mut().music_folder = Some(dir.clone());
        save_settings(&weak, &settings);
        scan_folder(weak.clone(), library.clone(), displayed.clone(), query.clone(), sort.clone(), scan_gen.clone(), dir);
    });

    // Spectrum rays: while the big player view is open, poll the analyzer ~40 times a second.
    // Hidden = the timer returns at once and the audio callback stops feeding the tap.
    let rays_timer = slint::Timer::default();
    {
        let weak = ui.as_weak();
        let tap = player.spectrum();
        let last = std::cell::Cell::new(std::time::Instant::now());
        let was_quiet = std::cell::Cell::new(false);
        let phase = std::cell::Cell::new(0.0_f32);
        rays_timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(25), move || {
            let Some(ui) = weak.upgrade() else { return };
            let now = std::time::Instant::now();
            let dt = now.duration_since(last.replace(now)).as_secs_f32();
            if !ui.get_show_player() {
                tap.pause();
                was_quiet.set(false);
                return;
            }
            let f = tap.poll(dt);
            let quiet = f.level < 0.002 && f.bass < 0.002 && f.peaks.iter().all(|p| *p < 0.002);
            if quiet && was_quiet.get() {
                return; // nothing moves: keep the idle ticks, spend no CPU
            }
            was_quiet.set(quiet);
            // The picture turns slowly, a bit faster when the music is loud.
            phase.set((phase.get() + dt * (0.008 + 0.03 * f.level)).rem_euclid(1.0));
            let r = rays::build(&f.bands, &f.peaks, phase.get());
            ui.set_rays_halo(r.halo.into());
            ui.set_rays_body(r.body.into());
            ui.set_rays_tips(r.tips.into());
            ui.set_rays_peaks(r.peaks.into());
            ui.set_pulse(f.bass);
        });
    }

    ui.run()
}
