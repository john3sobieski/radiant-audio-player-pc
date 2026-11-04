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
use radiant_core::models::{SortKey, SortState, Track};
use slint::{Model, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
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

/// Copies the current effect settings to the engine and to the settings file.
/// The `fx` borrow ends before `settings` is borrowed, so the two cells never overlap.
fn commit_fx(
    weak: &slint::Weak<MainWindow>,
    player: &Player,
    fx: &RefCell<AudioFxSettings>,
    settings: &RefCell<AppSettings>,
) {
    let snapshot = fx.borrow().clone();
    player.send(Command::SetFx(snapshot.clone()));
    settings.borrow_mut().fx = snapshot;
    save_settings(weak, settings);
}

fn apply_settings(
    ui: &MainWindow,
    player: &Rc<Player>,
    fx: &Rc<RefCell<AudioFxSettings>>,
    eq_model: &Rc<VecModel<f32>>,
    settings: &AppSettings,
) {
    ui.set_volume(settings.volume);
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
    let bytes = radiant_core::library::read_cover(path)?;
    let thumb = image::load_from_memory(&bytes).ok()?.thumbnail(96, 96).to_rgba8();
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
    let needle = query.lock().map(|q| q.to_lowercase()).unwrap_or_default();
    let visible: Vec<_> = tracks.iter().filter(|track| {
        needle.is_empty()
            || track.title.to_lowercase().contains(&needle)
            || track.artist.as_deref().unwrap_or("").to_lowercase().contains(&needle)
            || track.album.as_deref().unwrap_or("").to_lowercase().contains(&needle)
    }).collect();
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
    let player = Rc::new(Player::spawn(move |status| {
        // Position of the playing track in the list (-1 = none), for the row highlight.
        let index = status
            .current
            .as_ref()
            .and_then(|c| status_library.lock().ok()?.iter().position(|p| p == c))
            .map_or(-1, |i| i as i32);
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_current_index(index);
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

    ui.run()
}
