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
    slider_to_speed, AudioFxSettings, Command, PlayState, Player, RepeatMode, SpectrumTap, EQ_PRESETS, SPECTRUM_BANDS,
};
use radiant_core::applock::{self, CodeError, CodeKind, LockStore, RecoverResult, UnlockResult};
use radiant_core::settings::{self, AppSettings, EqMode, RepeatModeSetting};
mod eq_curve;
mod i18n;
mod rays;

use radiant_core::models::{AudioInfo, Playlist, PlaylistFolder, SortKey, SortState, Track, FAVORITES_ID, STEMS_FOLDER_ID};
use radiant_core::playlists::{self, ListEntry, PlaylistSort, PlaylistSortKey, PlaylistStore};
use radiant_core::queues::{self, QueueStore};
use slint::{Model, ModelRc, SharedString, VecModel};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

/// Switches the interface language: the Rust-side texts (`i18n`) and the bundled Slint translation of `@tr`
/// texts. `setting` is "system" or a language code. Returns the language code that is active now.
fn apply_language(setting: &str) -> &'static str {
    let code = i18n::set_language(setting);
    // Every language has a .po file (English too), so this only fails when the files are missing.
    let _ = slint::select_bundled_translation(code);
    code
}

/// The Rust-made texts that stay on screen: the names of the equalizer presets.
fn refresh_translated_texts(ui: &MainWindow) {
    let names: Vec<SharedString> = EQ_PRESETS.iter().map(|p| i18n::tr(p.name).into()).collect();
    ui.set_eq_preset_names(ModelRc::new(VecModel::from(names)));
    // The model card's status line is a ready-made text: rebuild it in the new language (not while a download runs:
    // its progress events rebuild the text by themselves).
    if !matches!(ui.get_stem_state(), 1 | 2) {
        show_stem_state(ui);
    }
}

fn save_settings(weak: &slint::Weak<MainWindow>, settings: &RefCell<AppSettings>) {
    let snapshot = settings.borrow().clone();
    if let Err(e) = settings::save(&snapshot) {
        if let Some(ui) = weak.upgrade() {
            ui.set_status(i18n::tr_fmt("Could not save settings: {}", &[&e]).into());
        }
    }
}

// ---- AI & stems: the Demucs model file (stage 11). The files and the download live in `radiant-stems`; here only the
// settings card is driven. `stem-state`: 0 not downloaded, 1 downloading, 2 checking the file, 3 ready,
// 4 this build has no verified model address (download off).

/// The models folder: `%APPDATA%/Radiant/models` (next to `settings.json`).
fn stem_store() -> radiant_stems::ModelStore {
    let dir = settings::settings_path().parent().map(|p| p.join("models")).unwrap_or_else(|| PathBuf::from("models"));
    radiant_stems::ModelStore::new(dir)
}

/// Bytes -> whole megabytes (decimal, as file managers show them).
fn stem_mb(bytes: u64) -> String {
    ((bytes + 500_000) / 1_000_000).to_string()
}

/// Shows what is on disk in the settings card.
fn show_stem_state(ui: &MainWindow) {
    use radiant_stems::{ModelState, MODEL};
    let store = stem_store();
    let (state, info) = if store.state() == ModelState::Ready {
        let size = std::fs::metadata(store.model_path()).map(|m| m.len()).unwrap_or(MODEL.approx_bytes);
        (3, i18n::tr_fmt("Ready · {} MB", &[&stem_mb(size)]))
    } else if !MODEL.is_pinned() {
        (4, i18n::tr("Not available yet: this build has no verified model address"))
    } else {
        (0, i18n::tr("Not downloaded"))
    };
    ui.set_stem_state(state);
    ui.set_stem_progress(0.0);
    ui.set_stem_info(info.into());
}

/// Runs a download / import on a worker thread; progress and the result come back to the window. Only one job at
/// a time (`busy`); `cancel` stops it (a half-loaded download is kept and continued next time).
/// `mismatch_text` is the message for a file that is not the expected model.
fn run_stem_job<F>(
    weak: &slint::Weak<MainWindow>,
    cancel: &Arc<AtomicBool>,
    busy: &Arc<AtomicBool>,
    mismatch_text: &'static str,
    job: F,
) where
    F: FnOnce(
            &radiant_stems::ModelStore,
            &AtomicBool,
            &mut dyn FnMut(radiant_stems::Progress),
        ) -> Result<(), radiant_stems::StemError>
        + Send
        + 'static,
{
    use radiant_stems::{Progress, StemError};
    if busy.swap(true, Ordering::SeqCst) {
        return;
    }
    cancel.store(false, Ordering::SeqCst);
    if let Some(ui) = weak.upgrade() {
        ui.set_stem_state(1);
        ui.set_stem_progress(0.0);
        ui.set_stem_info(i18n::tr_fmt("Downloading… {} of {} MB", &[&0, &stem_mb(radiant_stems::MODEL.approx_bytes)]).into());
    }
    let (weak, cancel, busy) = (weak.clone(), cancel.clone(), busy.clone());
    std::thread::spawn(move || {
        let store = stem_store();
        let mut last: Option<std::time::Instant> = None;
        let mut on_progress = |p: Progress| {
            let now = std::time::Instant::now();
            if last.map_or(false, |t| now.duration_since(t) < std::time::Duration::from_millis(120)) {
                return;
            }
            last = Some(now);
            let (state, done, total) = match p {
                Progress::Download { done, total } => (1, done, total),
                Progress::Verify { done, total } => (2, done, total),
            };
            let _ = weak.upgrade_in_event_loop(move |ui| {
                ui.set_stem_state(state);
                ui.set_stem_progress(if total > 0 { done as f32 / total as f32 } else { 0.0 });
                let info = if state == 1 {
                    i18n::tr_fmt("Downloading… {} of {} MB", &[&stem_mb(done), &stem_mb(total)])
                } else {
                    i18n::tr("Checking the file…")
                };
                ui.set_stem_info(info.into());
            });
        };
        let result = job(&store, &*cancel, &mut on_progress);
        busy.store(false, Ordering::SeqCst);
        let _ = weak.upgrade_in_event_loop(move |ui| {
            let message = match &result {
                Ok(()) => i18n::tr("The model is ready"),
                Err(StemError::Cancelled) => i18n::tr("Download cancelled"),
                Err(StemError::NotPinned) => i18n::tr("Not available yet: this build has no verified model address"),
                Err(StemError::Mismatch) => i18n::tr(mismatch_text),
                Err(e) => i18n::tr_fmt("Model download failed: {}", &[e]),
            };
            show_stem_state(&ui);
            ui.set_status(message.into());
        });
    });
}

// ---- Stems: one separation at a time, started from the context menu ("Create stems": four files) or from the Stems
// tab of the player (the chosen parts as one file, optionally each part too). `busy` = a job runs, `cancel` stops it.
#[derive(Clone)]
struct StemsCtl {
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

impl StemsCtl {
    fn new() -> Self {
        Self { busy: Arc::new(AtomicBool::new(false)), cancel: Arc::new(AtomicBool::new(false)) }
    }
}

/// `Artist - Title` for the file names (just the title without an artist; the file name without a title).
fn stems_base(path: &Path, artist: Option<&str>, title: &str) -> String {
    let title = if title.trim().is_empty() {
        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    } else {
        title.to_string()
    };
    match artist.filter(|a| !a.trim().is_empty()) {
        Some(a) => format!("{a} - {title}"),
        None => title,
    }
}

/// The track of the Stems tab: path and `Artist - Title` of what is playing (set by `show_now_playing`).
static NOW_STEMS_TRACK: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);

/// Finished jobs waiting to be put into the Stems folder of the playlists: (`Artist - Title`, the written files).
/// The worker thread pushes here and calls `stems-finished`; the handler (UI thread) owns the playlists.
static STEMS_DONE: Mutex<Vec<(String, Vec<PathBuf>)>> = Mutex::new(Vec::new());

/// The track picked in the context menu for the "Create stems" window: path and `Artist - Title`.
static STEMS_MENU_TARGET: Mutex<Option<(PathBuf, String)>> = Mutex::new(None);

/// The parts chosen on the Stems tab and "also save each part separately".
fn stems_choice(ui: &MainWindow) -> (radiant_stems::StemSet, bool) {
    (
        radiant_stems::StemSet::from_flags(ui.get_stems_vocals(), ui.get_stems_drums(), ui.get_stems_bass(), ui.get_stems_other()),
        ui.get_stems_separate(),
    )
}

/// The line under the part switches: which file(s) the current choice makes (or what is wrong with the choice).
fn update_stems_file_info(ui: &MainWindow) {
    let (selection, each_part) = stems_choice(ui);
    let sets = radiant_stems::output_sets(selection, each_part);
    let text = if sets.is_empty() {
        i18n::tr("Pick one to three parts, or turn on separate files")
    } else {
        // The window of the context menu works on the track it was opened for, the tab on the playing one.
        let target = if ui.get_show_stems_popup() { &STEMS_MENU_TARGET } else { &NOW_STEMS_TRACK };
        let base = target.lock().ok().and_then(|t| t.as_ref().map(|(_, b)| b.clone()));
        match base {
            Some(base) => {
                let names: Vec<String> =
                    sets.iter().filter_map(|set| radiant_stems::job::file_name(&base, *set)).collect();
                match names.as_slice() {
                    [one] => i18n::tr_fmt("Will be saved as: {}", &[one]),
                    [first, ..] => i18n::tr_fmt("Will be saved: {} files (first: {})", &[&names.len(), first]),
                    [] => String::new(),
                }
            }
            None => String::new(),
        }
    };
    ui.set_stems_file_info(text.into());
}

/// Checks that apply to every start (low-power mode, the model, an empty choice), makes the folder name and starts.
/// A refusal goes to the status line and to the Stems tab.
fn begin_stems(
    weak: &slint::Weak<MainWindow>,
    settings: &RefCell<AppSettings>,
    ctl: &StemsCtl,
    path: PathBuf,
    base: String,
    sets: Vec<radiant_stems::StemSet>,
) {
    let (low_power, gpu, folder) = {
        let s = settings.borrow();
        (s.low_power, s.stems_gpu, s.music_folder.clone())
    };
    let refuse = |text: String| {
        if let Some(ui) = weak.upgrade() {
            ui.set_status(text.clone().into());
            ui.set_stems_info(text.into());
        }
    };
    if radiant_stems::job::is_stem_file(&path) {
        return refuse(i18n::tr("Stems cannot be made from stems"));
    }
    if !radiant_stems::allowed(low_power) {
        return refuse(i18n::tr("Stems are not available in low-power mode"));
    }
    if stem_store().state() != radiant_stems::ModelState::Ready {
        return refuse(i18n::tr("Download the separation model first (Settings → AI & stems)"));
    }
    if sets.is_empty() {
        return refuse(i18n::tr("Pick one to three parts, or turn on separate files"));
    }
    if ctl.busy.load(Ordering::SeqCst) {
        return;
    }
    let root = folder.or_else(|| path.parent().map(Path::to_path_buf)).unwrap_or_default();
    let out_dir = root.join("Stems").join(radiant_stems::job::safe_file_name(&base));
    start_stems_job(weak, ctl, path, base, out_dir, sets, gpu);
}

/// Runs the separation on a worker thread; progress (with the time left) and the result go to the status line and
/// to the Stems tab. `gpu` asks for DirectML (the job falls back to the CPU and says so in the result).
fn start_stems_job(
    weak: &slint::Weak<MainWindow>,
    ctl: &StemsCtl,
    input: PathBuf,
    base: String,
    out_dir: PathBuf,
    sets: Vec<radiant_stems::StemSet>,
    gpu: bool,
) {
    use radiant_stems::StemError;
    if ctl.busy.swap(true, Ordering::SeqCst) {
        return;
    }
    ctl.cancel.store(false, Ordering::SeqCst);
    if let Some(ui) = weak.upgrade() {
        let text = i18n::tr_fmt("Creating stems… {}%", &[&0]);
        ui.set_stems_busy(true);
        ui.set_stems_progress(0.0);
        ui.set_stems_info(text.clone().into());
        ui.set_status(text.into());
    }
    let (weak, ctl) = (weak.clone(), ctl.clone());
    std::thread::spawn(move || {
        let mut eta = radiant_stems::eta::Eta::new();
        let mut last = (-1_i32, u64::MAX);
        let result = radiant_stems::create_stems(
            &input,
            &stem_store().model_path(),
            &out_dir,
            &base,
            &sets,
            gpu,
            &ctl.cancel,
            &mut |p| {
                let pct = (p * 100.0) as i32;
                let left = eta.update(p);
                // The interface is told only when the percent or the shown time changes.
                if (pct, left.unwrap_or(u64::MAX)) == last || ctl.cancel.load(Ordering::SeqCst) {
                    return;
                }
                last = (pct, left.unwrap_or(u64::MAX));
                let text = match left {
                    Some(secs) => i18n::tr_fmt(
                        "Creating stems… {}% · {} left",
                        &[&pct, &radiant_stems::eta::format_secs(secs)],
                    ),
                    None => i18n::tr_fmt("Creating stems… {}%", &[&pct]),
                };
                let _ = weak.upgrade_in_event_loop(move |ui| {
                    ui.set_stems_progress(p.clamp(0.0, 1.0));
                    ui.set_stems_info(text.clone().into());
                    ui.set_status(text.into());
                });
            },
        );
        ctl.busy.store(false, Ordering::SeqCst);
        let _ = weak.upgrade_in_event_loop(move |ui| {
            let message = match &result {
                Ok(done) => {
                    // Into the playlist of this track in the Stems folder (done by the UI-thread handler).
                    if let Ok(mut list) = STEMS_DONE.lock() {
                        list.push((base.clone(), done.files.clone()));
                    }
                    ui.invoke_stems_finished();
                    let mut m = i18n::tr_fmt("Stems saved to {}", &[&out_dir.display()]);
                    if gpu && !done.gpu {
                        m.push(' ');
                        m.push_str(&i18n::tr("(the graphics card could not be used: the processor did the work)"));
                    }
                    m
                }
                Err(StemError::Cancelled) => i18n::tr("Stem separation cancelled"),
                Err(e) => i18n::tr_fmt("Stem separation failed: {}", &[e]),
            };
            ui.set_stems_busy(false);
            ui.set_stems_progress(0.0);
            ui.set_stems_info(message.clone().into());
            ui.set_status(message.into());
        });
    });
}

/// Own accent color: the HSV sliders in the settings and the `Theme` colors.
fn apply_custom_color(ui: &MainWindow, hsv: [f32; 3]) {
    let rgb = radiant_core::theme::hsv_to_rgb(hsv[0], hsv[1], hsv[2]);
    let theme = ui.global::<Theme>();
    theme.set_custom(slint::Color::from_rgb_u8(rgb.0, rgb.1, rgb.2));
    // Text on the accent: dark on light colors (yellow...), white otherwise.
    let ink = if radiant_core::theme::needs_dark_text(rgb) {
        slint::Color::from_rgb_u8(0x1a, 0x14, 0x05)
    } else {
        slint::Color::from_rgb_u8(0xff, 0xff, 0xff)
    };
    theme.set_custom_ink(ink);
    ui.set_cu_h(hsv[0]);
    ui.set_cu_s(hsv[1]);
    ui.set_cu_v(hsv[2]);
}

/// Pushes the saved own colors to the settings page (small circles).
fn refresh_saved_colors(ui: &MainWindow, colors: &[[f32; 3]]) {
    let model: Vec<slint::Color> = colors
        .iter()
        .map(|c| {
            let (r, g, b) = radiant_core::theme::hsv_to_rgb(c[0], c[1], c[2]);
            slint::Color::from_rgb_u8(r, g, b)
        })
        .collect();
    ui.set_saved_colors(ModelRc::new(VecModel::from(model)));
}

/// Spectrum rays: while the big player view is open, poll the analyzer ~40 times a second.
/// Hidden = the timer returns at once and the audio callback stops feeding the tap.
/// Not used in low-power mode (see `apply_low_power_visuals`): the timer is stopped, so it never wakes the app.
fn start_rays_timer(timer: &slint::Timer, weak: slint::Weak<MainWindow>, tap: SpectrumTap) {
    let last = std::cell::Cell::new(std::time::Instant::now());
    let was_quiet = std::cell::Cell::new(false);
    let phase = std::cell::Cell::new(0.0_f32);
    let clock = std::cell::Cell::new(0.0_f32);
    timer.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(25), move || {
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
        // Seconds for the sway / flicker of the rays (wrapped, so f32 stays precise).
        clock.set((clock.get() + dt).rem_euclid(3600.0));
        let energy = (f.level * 1.5).min(1.0);
        let r = rays::build(&f.bands, &f.peaks, phase.get(), clock.get(), f.bass, energy);
        ui.set_rays_halo(r.halo.into());
        ui.set_rays_body(r.body.into());
        ui.set_rays_tips(r.tips.into());
        ui.set_rays_peaks(r.peaks.into());
        ui.set_pulse(f.bass);
        ui.set_energy(energy);
    });
}

/// One calm, motionless picture of the rays (low-power mode): built once, nothing updates it afterwards.
fn show_still_rays(ui: &MainWindow) {
    let level = [0.2_f32; SPECTRUM_BANDS];
    let r = rays::build(&level, &level, 0.0, 0.0, 0.0, 0.0);
    ui.set_rays_halo(r.halo.into());
    ui.set_rays_body(r.body.into());
    ui.set_rays_tips(r.tips.into());
    ui.set_rays_peaks(r.peaks.into());
    ui.set_pulse(0.0);
    ui.set_energy(0.0);
}

/// The UI side of low-power mode that Rust owns: the spectrum rays. On = the timer is stopped (no wake-ups at all),
/// the analyzer is switched off and the rays show a still picture. Off = the live rays are back.
/// (Animations are switched by `Theme.low-power` in `main.slint`, the sound effects by `Command::SetLowPower`.)
fn apply_low_power_visuals(weak: &slint::Weak<MainWindow>, timer: &slint::Timer, player: &Player, on: bool) {
    let tap = player.spectrum();
    if on {
        timer.stop();
        tap.pause();
        if let Some(ui) = weak.upgrade() {
            show_still_rays(&ui);
        }
    } else {
        start_rays_timer(timer, weak.clone(), tap);
    }
}

fn apply_settings(
    ui: &MainWindow,
    player: &Rc<Player>,
    fx: &Rc<RefCell<AudioFxSettings>>,
    eq_model: &Rc<VecModel<f32>>,
    settings: &AppSettings,
) {
    {
        let theme = ui.global::<Theme>();
        theme.set_mode(settings.theme as i32);
        theme.set_sel_light(settings.palettes[0] as i32);
        theme.set_sel_dark(settings.palettes[1] as i32);
        theme.set_sel_black(settings.palettes[2] as i32);
    }
    // Low-power mode: the UI flag (animations) and the engine (sound effects, wake-ups) before anything else is set.
    ui.global::<Theme>().set_low_power(settings.low_power);
    player.send(Command::SetLowPower(settings.low_power));
    ui.set_settings_section(settings.settings_section as i32);
    apply_custom_color(ui, settings.custom_hsv);
    refresh_saved_colors(ui, &settings.saved_colors);
    ui.set_volume(settings.volume);
    ui.set_section(settings.section as i32);
    ui.set_queue_index(settings.queue_index as i32);
    ui.set_pl_sort_mode(settings.playlist_sort as i32);
    ui.set_pl_sort_descending(settings.playlist_sort_desc);
    ui.set_album_sort_mode(settings.album_sort as i32);
    ui.set_album_sort_descending(settings.album_sort_desc);
    ui.set_shuffle(settings.shuffle);
    ui.set_repeat_mode(repeat_mode_to_ui(settings.repeat_mode));
    ui.set_speed_t(settings.speed_t);
    ui.set_fx_normalize(settings.fx.normalize);
    ui.set_fx_mono(settings.fx.mono);
    ui.set_fx_bass(settings.fx.bass_db);
    ui.set_fx_treble(settings.fx.treble_db);
    ui.set_fx_subsonic(settings.fx.subsonic_percent);
    // Nothing is playing yet, so a per-track curve is flat here; it is loaded when a track starts.
    refresh_eq(ui, player, fx, eq_model, settings, None);
    player.send(Command::SetVolume(settings.volume));
    player.send(Command::SetShuffle(settings.shuffle));
    player.send(Command::SetRepeat(repeat_mode_to_engine(settings.repeat_mode)));
    player.send(Command::SetSpeed(slider_to_speed(settings.speed_t)));
}

/// Shows the equalizer state in the panel: mode, whether it can be edited, the track the curve belongs to
/// (per-track mode), the small response graph and the highlighted preset.
fn show_eq_state(ui: &MainWindow, settings: &AppSettings, track: Option<&Path>, gains: &[f32; 12]) {
    ui.set_eq_mode(settings.eq_mode.to_index());
    ui.set_eq_editable(settings.eq_editable(track));
    // Presets work in every mode (Off switches the equalizer on); per-track mode needs a track to give it to.
    ui.set_eq_presets_enabled(settings.eq_mode != EqMode::PerTrack || track.is_some());
    let name = track.and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    ui.set_eq_track_name(name.into());
    ui.set_eq_track_saved(settings.has_track_eq(track));
    let (line, fill) = eq_curve::paths(gains);
    ui.set_eq_curve_line(line.into());
    ui.set_eq_curve_fill(fill.into());
    let preset = EQ_PRESETS.iter().position(|p| &p.gains == gains && settings.eq_mode != EqMode::Off);
    ui.set_eq_active_preset(preset.map_or(-1, |i| i as i32));
}

/// Makes the curve of the current mode and track audible and visible (sliders, graph, engine). In per-track
/// mode this also loads the speed, bass, treble and subsonic of the track.
/// Called on start, when the mode changes and when another track starts.
fn refresh_eq(
    ui: &MainWindow,
    player: &Rc<Player>,
    fx: &Rc<RefCell<AudioFxSettings>>,
    eq_model: &Rc<VecModel<f32>>,
    settings: &AppSettings,
    track: Option<&Path>,
) {
    let gains = settings.effective_eq(track);
    let sound = settings.effective_sound(track);
    for (i, g) in gains.iter().enumerate() {
        eq_model.set_row_data(i, *g);
    }
    {
        let mut s = fx.borrow_mut();
        *s = settings.fx.clone();
        s.eq_gains_db = gains;
        s.bass_db = sound.bass_db;
        s.treble_db = sound.treble_db;
        s.subsonic_percent = sound.subsonic_percent;
        player.send(Command::SetFx(s.clone()));
    }
    ui.set_fx_bass(sound.bass_db);
    ui.set_fx_treble(sound.treble_db);
    ui.set_fx_subsonic(sound.subsonic_percent);
    ui.set_speed_t(sound.speed_t);
    // The engine ignores a speed it already has.
    player.send(Command::SetSpeed(slider_to_speed(sound.speed_t)));
    show_eq_state(ui, settings, track, &gains);
}

/// A slider or preset wants `gains` as the active curve (global, or the one of the current track).
/// Refused (and the sliders snap back) when the mode does not allow editing.
fn apply_curve(
    weak: &slint::Weak<MainWindow>,
    player: &Rc<Player>,
    fx: &Rc<RefCell<AudioFxSettings>>,
    eq_model: &Rc<VecModel<f32>>,
    settings: &Rc<RefCell<AppSettings>>,
    track: Option<&Path>,
    gains: [f32; 12],
) {
    let ok = settings.borrow_mut().set_active_eq(track, gains);
    let shown = if ok { gains } else { settings.borrow().effective_eq(track) };
    for (i, g) in shown.iter().enumerate() {
        eq_model.set_row_data(i, *g);
    }
    if ok {
        let mut s = fx.borrow_mut();
        s.eq_gains_db = gains;
        player.send(Command::SetFx(s.clone()));
    }
    if let Some(ui) = weak.upgrade() {
        show_eq_state(&ui, &settings.borrow(), track, &shown);
    }
    if ok {
        save_settings(weak, settings);
    }
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
    use i18n::tr;
    let mut rows: Vec<(String, String)> = Vec::new();
    // Tags first, then technical data, then the file itself.
    rows.push((tr("Title"), t.title.clone()));
    rows.push((tr("Artist"), t.artist.clone().unwrap_or_default()));
    rows.push((tr("Album artist"), t.album_artist.clone().unwrap_or_default()));
    rows.push((tr("Album"), t.album.clone().unwrap_or_default()));
    rows.push((tr("Track no."), t.track_no.filter(|n| *n > 0).map(|n| n.to_string()).unwrap_or_default()));
    rows.push((tr("Year"), t.year.map(|y| y.to_string()).unwrap_or_default()));
    rows.push((tr("Format"), t.info.format.clone()));
    rows.push((tr("Bitrate"), t.info.bitrate_kbps.filter(|k| *k > 0).map(|k| format!("{k} kbps")).unwrap_or_default()));
    rows.push((tr("Sample rate"), t.info.sample_rate.map(|r| format!("{} kHz", r as f32 / 1000.0)).unwrap_or_default()));
    rows.push((tr("Channels"), match t.info.channels {
        Some(1) => tr("Mono"),
        Some(2) => tr("Stereo"),
        Some(n) if n > 0 => i18n::tr_n("{n} channel", "{n} channels", n as usize),
        _ => String::new(),
    }));
    rows.push((tr("Duration"), t.duration_ms.filter(|d| *d > 0).map(fmt_time).unwrap_or_default()));
    rows.push((tr("Size"), if t.info.size_bytes > 0 { fmt_size(t.info.size_bytes) } else { String::new() }));
    rows.push((tr("Lyrics"), if t.lyrics_path.is_some() { tr("File next to the track") }
        else if t.tag_lyrics { tr("Embedded in tags") } else { String::new() }));
    rows.push((tr("File"), t.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
    rows.push((tr("Folder"), t.path.parent().map(|p| p.display().to_string()).unwrap_or_default()));
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
    if let Ok(mut t) = NOW_STEMS_TRACK.lock() {
        *t = Some((track.path.clone(), stems_base(&track.path, track.artist.as_deref(), &track.title)));
    }
    update_stems_file_info(ui);

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
    ui.set_lyr_user_scroll(false); // a new track starts with the auto-follow on
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
    fresh: bool,
) {
    // A slower scan must not overwrite the list from a newer one.
    let ticket = scan_gen.fetch_add(1, Ordering::AcqRel) + 1;
    if let Some(ui) = weak.upgrade() {
        ui.set_status(i18n::tr("Scanning...").into());
        ui.set_scanning(true);
    }
    std::thread::spawn(move || {
        let tracks = radiant_core::library::scan(&dir);
        let weak_for_ui = weak.clone();
        let _ = weak.upgrade_in_event_loop(move |ui| {
            if scan_gen.load(Ordering::Acquire) != ticket {
                return; // a newer scan is running and keeps the spinner
            }
            ui.set_scanning(false);
            // Manual rescan: covers may have changed on disk, so decode them again.
            if fresh {
                if let Ok(mut c) = COVER_CACHE.lock() { c.clear(); }
                if let Ok(mut c) = ALBUM_COVERS.lock() { c.clear(); }
            }
            if let Ok(mut l) = library.lock() { *l = tracks; }
            refresh_tracks(&weak_for_ui, &library, &displayed, &query, *sort.lock().unwrap_or_else(|e| e.into_inner()));
            // The queue page marks files that are not in the library: it needs the new list.
            ui.invoke_queue_page_refresh();
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
/// Sort of the album grid: (0 = manual, 1 = name, 2 = artist, 3 = duration; descending).
static ALBUM_SORT: Mutex<(u8, bool)> = Mutex::new((1, false));
/// Album keys in the order set by hand (used by the manual sort mode).
static ALBUM_ORDER: Mutex<Vec<String>> = Mutex::new(Vec::new());
/// Rows of the open album page (same order as the model): the file of a track, `None` for a "Disc N" caption.
static OPEN_ROWS: Mutex<Vec<Option<PathBuf>>> = Mutex::new(Vec::new());
static ALBUM_COVERS: Mutex<BTreeMap<String, Option<CoverPixels>>> = Mutex::new(BTreeMap::new());
/// Albums whose cover is being decoded right now (no duplicate work while a tile scrolls in and out).
static ALBUM_PENDING: Mutex<BTreeSet<String>> = Mutex::new(BTreeSet::new());

/// Text that is missing (or blank) goes last in both directions; the rest compares without case.
fn cmp_text_last(a: &Option<String>, b: &Option<String>, descending: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let clean = |t: &Option<String>| t.as_deref().filter(|v| !v.trim().is_empty()).map(str::to_lowercase);
    match (clean(a), clean(b)) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => if descending { y.cmp(&x) } else { x.cmp(&y) },
    }
}

/// Orders the album grid by the saved mode: manual (dragged order, new albums last), name, artist or total duration.
fn sort_albums(albums: &mut [radiant_core::models::Album], by_path: &HashMap<&Path, &Track>) {
    let (mode, descending) = ALBUM_SORT.lock().map(|s| *s).unwrap_or((1, false));
    let by_name = |a: &radiant_core::models::Album, b: &radiant_core::models::Album| {
        cmp_text_last(&a.name, &b.name, false).then_with(|| cmp_text_last(&a.artist, &b.artist, false))
    };
    match mode {
        0 => {
            let order = ALBUM_ORDER.lock().map(|o| o.clone()).unwrap_or_default();
            let pos: HashMap<&str, usize> = order.iter().enumerate().map(|(i, k)| (k.as_str(), i)).collect();
            albums.sort_by(|a, b| {
                let ia = pos.get(a.key.as_str()).copied().unwrap_or(usize::MAX);
                let ib = pos.get(b.key.as_str()).copied().unwrap_or(usize::MAX);
                ia.cmp(&ib).then_with(|| by_name(a, b))
            });
        }
        2 => albums.sort_by(|a, b| {
            cmp_text_last(&a.artist, &b.artist, descending)
                .then_with(|| cmp_text_last(&a.name, &b.name, false))
        }),
        3 => {
            let total = |a: &radiant_core::models::Album| -> u64 {
                a.tracks.iter().filter_map(|p| by_path.get(p.as_path()).and_then(|t| t.duration_ms)).sum()
            };
            albums.sort_by(|a, b| {
                let o = total(a).cmp(&total(b));
                let o = if descending { o.reverse() } else { o };
                o.then_with(|| by_name(a, b))
            });
        }
        _ => albums.sort_by(|a, b| {
            cmp_text_last(&a.name, &b.name, descending).then_with(|| cmp_text_last(&a.artist, &b.artist, false))
        }),
    }
}

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
    sort_albums(&mut albums, &by_path);

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
        let title = a.name.clone().unwrap_or_else(|| i18n::tr("Unknown album"));
        let (cover, has_cover) = match cache.get(&a.key) {
            Some(Some(px)) => (px.to_image(), true),
            _ => (slint::Image::default(), false),
        };
        rows.push(AlbumRow {
            letter: title.chars().next().map(|c| c.to_uppercase().collect::<String>()).unwrap_or_default().into(),
            title: title.into(),
            artist: a.artist.clone().unwrap_or_else(|| i18n::tr("Unknown artist")).into(),
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
            rows.push(AlbumTrackRow { header: i18n::tr_fmt("Disc {}", &[&disc]).into(), ..Default::default() });
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
    let mut meta = i18n::tr_n("{n} track", "{n} tracks", count);
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

/// Albums waiting for their cover decode (the newest request is served first: it is the one on screen).
static ALBUM_QUEUE: Mutex<Vec<(String, Vec<PathBuf>)>> = Mutex::new(Vec::new());
/// Number of running cover decoder threads (changed only while `ALBUM_QUEUE` is locked).
static ALBUM_WORKERS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// Decoder threads at most: a fast scroll used to start one thread per tile and starve the UI thread.
const ALBUM_WORKERS_MAX: usize = 2;
/// Waiting requests at most; older ones (tiles long scrolled past) are dropped and ask again when they return.
const ALBUM_QUEUE_MAX: usize = 48;

/// A tile came into view: make sure its cover is decoded and shown.
fn album_cover_wanted(weak: slint::Weak<MainWindow>, index: usize) {
    // The tile already carries its cover (from `refresh_albums` or an earlier decode): nothing to do.
    // Re-sending the same picture every time a tile scrolled into view re-uploaded its texture
    // and rebuilt the row, which made scrolling the grid stutter.
    if let Some(ui) = weak.upgrade() {
        if ui.get_albums().row_data(index).map_or(false, |r| r.has_cover) {
            return;
        }
    }
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
            queue_album_decode(weak, key, paths);
        }
    }
}

/// Queues the decode of one album cover and makes sure a small, fixed set of threads works through the queue.
fn queue_album_decode(weak: slint::Weak<MainWindow>, key: String, paths: Vec<PathBuf>) {
    use std::sync::atomic::Ordering as AtomicOrdering;
    let start_worker = {
        let Ok(mut queue) = ALBUM_QUEUE.lock() else {
            if let Ok(mut p) = ALBUM_PENDING.lock() { p.remove(&key); }
            return;
        };
        queue.push((key, paths));
        if queue.len() > ALBUM_QUEUE_MAX {
            let extra = queue.len() - ALBUM_QUEUE_MAX;
            let dropped: Vec<(String, Vec<PathBuf>)> = queue.drain(..extra).collect();
            if let Ok(mut p) = ALBUM_PENDING.lock() {
                for (k, _) in dropped { p.remove(&k); }
            }
        }
        if ALBUM_WORKERS.load(AtomicOrdering::Acquire) < ALBUM_WORKERS_MAX {
            ALBUM_WORKERS.fetch_add(1, AtomicOrdering::AcqRel);
            true
        } else {
            false
        }
    };
    if !start_worker {
        return;
    }
    std::thread::spawn(move || loop {
        // The worker count is lowered under the queue lock, so a request pushed right now is never left without a worker.
        let job = {
            let Ok(mut queue) = ALBUM_QUEUE.lock() else {
                ALBUM_WORKERS.fetch_sub(1, AtomicOrdering::AcqRel);
                return;
            };
            match queue.pop() {
                Some(job) => job,
                None => {
                    ALBUM_WORKERS.fetch_sub(1, AtomicOrdering::AcqRel);
                    return;
                }
            }
        };
        let (key, paths) = job;
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

/// Puts a decoded cover into the tile of its album (the position is looked up again: the grid may have changed).
fn apply_album_cover(ui: &MainWindow, key: &str, px: &CoverPixels) {
    let Some(i) = ALBUMS.lock().ok().and_then(|a| a.iter().position(|e| e.key == key)) else { return };
    let model = ui.get_albums();
    if let Some(mut row) = model.row_data(i) {
        // Already shown (a second request for the same album): do not upload the texture again.
        if row.has_cover {
            return;
        }
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
/// Tracks of the open queue page, same order as its rows.
static Q_ROWS: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

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
    if p.id == FAVORITES_ID { i18n::tr("Liked tracks") } else { p.name.clone() }
}

/// Shown name of a folder: the Stems folder is translated, the others are the user's own text.
fn folder_name(f: &PlaylistFolder) -> String {
    if f.id == STEMS_FOLDER_ID { i18n::tr("Stems") } else { f.name.clone() }
}

fn tracks_text(n: usize) -> String {
    i18n::tr_n("{n} track", "{n} tracks", n)
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
        .map(|i| FolderChip { id: st.store.folders[i].id.clone().into(), name: folder_name(&st.store.folders[i]).into() })
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
                    name: folder_name(f).into(),
                    locked: PlaylistStore::folder_is_locked(&f.id),
                    sub: i18n::tr_n("{n} playlist", "{n} playlists", n).into(),
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
                    ..Default::default()
                }
            }
        })
        .collect();
    ui.set_pl_rows(ModelRc::new(VecModel::from(rows)));
    ui.set_pl_total(st.store.playlists.len() as i32);
}

fn save_playlists(ui: &MainWindow, store: &PlaylistStore) {
    if let Err(e) = playlists::save(store) {
        ui.set_status(i18n::tr_fmt("Could not save playlists: {}", &[&e]).into());
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
                artist: i18n::tr("File not found").into(),
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

/// Fills the track header (cover, title, artist) shared by the "Add to playlist" and "Add to queue" windows.
fn set_popup_track(ui: &MainWindow, library: &Arc<Mutex<Vec<Track>>>, path: &Path) {
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
}

/// Opens the "Add to playlist" window for one track (menu of the Tracks list, of an album or of a playlist page).
/// The playlists that already hold the track are ticked; clicking a ticked one takes the track out again.
fn open_add_window(ui: &MainWindow, pl: &PlRef, library: &Arc<Mutex<Vec<Track>>>, path: PathBuf) {
    set_popup_track(ui, library, &path);
    let mut st = pl.borrow_mut();
    st.target = Some(path);
    st.moving = None;
    show_pl_popup(ui, &mut st, 0);
}

// ---- Queues ----

/// The four queues and the track the "Add to queue" window works on (UI thread only).
struct QState {
    store: QueueStore,
    target: Option<PathBuf>,
}

type QRef = Rc<RefCell<QState>>;

/// Pushes the queue state to the UI: track counts (window + section dots) and which queues hold the target.
fn refresh_queues(ui: &MainWindow, st: &QState) {
    let counts: Vec<i32> = st.store.counts().into_iter().map(|n| n as i32).collect();
    ui.set_queue_counts(ModelRc::new(VecModel::from(counts)));
    let added: Vec<bool> = (0..radiant_core::models::QUEUE_COUNT)
        .map(|i| st.target.as_deref().map_or(false, |t| st.store.contains(i, t)))
        .collect();
    ui.set_q_added(ModelRc::new(VecModel::from(added)));
}

fn save_queues(ui: &MainWindow, store: &QueueStore) {
    if let Err(e) = queues::save(store) {
        ui.set_status(i18n::tr_fmt("Could not save queues: {}", &[&e]).into());
    }
}

/// Opens the "Add to queue" window for one track. Queues that hold it are ticked; a click on a ticked one removes it.
fn open_queue_window(ui: &MainWindow, q: &QRef, library: &Arc<Mutex<Vec<Track>>>, path: PathBuf) {
    set_popup_track(ui, library, &path);
    let mut st = q.borrow_mut();
    st.target = Some(path);
    refresh_queues(ui, &st);
    ui.set_show_q_popup(true);
}

/// Fills the queue page with the tracks of the queue that is open (`queue-index`).
fn fill_queue_page(ui: &MainWindow, st: &QState, library: &Arc<Mutex<Vec<Track>>>) {
    let q = ui.get_queue_index().clamp(0, 3) as usize;
    let paths = st.store.tracks(q);
    let tracks = library.lock().map(|l| l.clone()).unwrap_or_default();
    let by_path: HashMap<&Path, &Track> = tracks.iter().map(|t| (t.path.as_path(), t)).collect();
    let mut rows: Vec<PlTrackRow> = Vec::with_capacity(paths.len());
    let mut total_ms = 0_u64;
    for (i, path) in paths.iter().enumerate() {
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
                artist: i18n::tr("File not found").into(),
                duration: SharedString::default(),
                missing: true,
            }),
        }
    }
    let mut meta = tracks_text(paths.len());
    if total_ms > 0 {
        meta.push_str(&format!(" · {}", fmt_total(total_ms)));
    }
    let playing = LAST_NOW.lock().ok().and_then(|l| l.clone());
    let current = playing.and_then(|c| paths.iter().position(|t| *t == c)).map_or(-1, |i| i as i32);
    if let Ok(mut open) = Q_ROWS.lock() { *open = paths.to_vec(); }
    ui.set_queue_page_meta(meta.into());
    ui.set_queue_current_index(current);
    ui.set_queue_page_tracks(ModelRc::new(VecModel::from(rows)));
}

/// Tracks of the open queue that are still in the library, in queue order.
fn queue_playable(st: &QState, ui: &MainWindow, library: &Arc<Mutex<Vec<Track>>>) -> Vec<PathBuf> {
    let q = ui.get_queue_index().clamp(0, 3) as usize;
    let known: HashSet<PathBuf> = library.lock().map(|l| l.iter().map(|t| t.path.clone()).collect()).unwrap_or_default();
    st.store.tracks(q).iter().filter(|t| known.contains(*t)).cloned().collect()
}

/// Shows the playlist window in `view` (0 = pick, 1 = create, 2 = move to folder).
fn show_pl_popup(ui: &MainWindow, st: &mut PlState, view: i32) {
    refresh_playlists(ui, st);
    ui.set_pl_has_track(st.target.is_some());
    ui.set_pl_name("".into());
    ui.set_pl_view(view);
    ui.set_show_pl_popup(true);
}

// ---------------------------------------------------------------------------------------------
// App lock (stage 10). The logic is in `radiant-core/src/applock.rs`; here are the screens' glue.
// "Background" on a PC = the window is minimized: it locks at that moment, so the code is asked on return.
// (Losing focus does not count: file dialogs and other windows take the focus all the time.)
// ---------------------------------------------------------------------------------------------

struct LockCtl {
    store: RefCell<LockStore>,
    /// A new code + recovery phrase being set up: becomes the real store only after the recovery file is saved.
    pending: RefCell<Option<(LockStore, String)>>,
    /// The first entry of a new code, waiting for the repeat.
    new_code: RefCell<Option<String>>,
    /// One-second countdown while the lock screen waits after too many wrong codes.
    tick: slint::Timer,
    /// Watches for the window being minimized (runs only while the lock is on).
    watch: slint::Timer,
    was_min: Cell<bool>,
}

fn fmt_wait(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn code_error_text(kind: CodeKind, e: CodeError) -> String {
    match e {
        CodeError::NotDigits => i18n::tr("Digits only."),
        CodeError::WrongLength => {
            if kind == CodeKind::Pin6 { i18n::tr("The code must be 6 digits.") } else { i18n::tr("The code must be 4 digits.") }
        }
        CodeError::CustomLength => i18n::tr("The code must be 4 to 64 characters."),
        CodeError::CustomChars => i18n::tr("Use Latin letters and digits only, no spaces."),
    }
}

fn save_lock(ui: &MainWindow, store: &LockStore) {
    if let Err(e) = applock::save(store) {
        ui.set_status(i18n::tr_fmt("Could not save the lock settings: {}", &[&e]).into());
    }
}

/// Shows the stored state in the UI: lock on / off and the kind of the code in use.
fn sync_lock_flags(ui: &MainWindow, store: &LockStore) {
    ui.set_lock_enabled(store.is_enabled());
    ui.set_lock_kind(store.kind.map_or(0, |k| k.to_index()));
}

/// Lock screen: waiting after too many wrong codes (entry disabled, countdown in the message) or free to try.
fn update_lock_wait(ctl: &Rc<LockCtl>, weak: &slint::Weak<MainWindow>, ui: &MainWindow) {
    let left = ctl.store.borrow().lock_remaining(applock::unix_now());
    if left > 0 {
        ui.set_lock_blocked(true);
        ui.set_lock_message(i18n::tr_fmt("Too many wrong codes. Try again in {}", &[&fmt_wait(left)]).into());
        if !ctl.tick.running() {
            let (ctl2, weak2) = (ctl.clone(), weak.clone());
            ctl.tick.start(slint::TimerMode::Repeated, std::time::Duration::from_secs(1), move || {
                if let Some(ui) = weak2.upgrade() {
                    update_lock_wait(&ctl2, &weak2, &ui);
                }
            });
        }
    } else {
        ctl.tick.stop();
        if ui.get_lock_blocked() {
            ui.set_lock_blocked(false);
            ui.set_lock_message(SharedString::default());
        }
    }
}

/// Shows the lock screen (only if a code is set).
fn lock_now(ctl: &Rc<LockCtl>, weak: &slint::Weak<MainWindow>, ui: &MainWindow) {
    if !ctl.store.borrow().is_enabled() || ui.get_locked() {
        return;
    }
    // A half-made code is thrown away.
    ctl.pending.borrow_mut().take();
    ctl.new_code.borrow_mut().take();
    ui.set_show_sec_popup(false);
    ui.set_sec_code(SharedString::default());
    ui.set_sec_phrase(SharedString::default());
    ui.set_lock_code(SharedString::default());
    ui.set_lock_phrase(SharedString::default());
    ui.set_lock_message(SharedString::default());
    ui.set_lock_recovering(false);
    ui.set_locked(true);
    update_lock_wait(ctl, weak, ui);
}

/// Starts watching the window: minimizing it locks the app.
fn start_lock_watch(ctl: &Rc<LockCtl>, weak: &slint::Weak<MainWindow>) {
    if ctl.watch.running() {
        return;
    }
    ctl.was_min.set(false);
    let (ctl2, weak2) = (ctl.clone(), weak.clone());
    ctl.watch.start(slint::TimerMode::Repeated, std::time::Duration::from_millis(500), move || {
        let Some(ui) = weak2.upgrade() else { return };
        let minimized = slint::ComponentHandle::window(&ui).is_minimized();
        if minimized && !ctl2.was_min.get() {
            lock_now(&ctl2, &weak2, &ui);
        }
        ctl2.was_min.set(minimized);
    });
}

/// "Forgot code?": checks the typed words or the text of the recovery file. On success the lock is turned off
/// completely (the owner can turn it on again with a new code and a new file).
fn do_recover(ctl: &Rc<LockCtl>, ui: &MainWindow, typed: &str) {
    let result = {
        let mut st = ctl.store.borrow_mut();
        let r = st.recover(typed);
        if r == RecoverResult::Ok {
            st.clear();
            save_lock(ui, &st);
        }
        r
    };
    match result {
        RecoverResult::Ok => {
            ctl.tick.stop();
            ctl.watch.stop();
            sync_lock_flags(ui, &ctl.store.borrow());
            ui.set_lock_blocked(false);
            ui.set_lock_message(SharedString::default());
            ui.set_lock_phrase(SharedString::default());
            ui.set_lock_code(SharedString::default());
            ui.set_lock_recovering(false);
            ui.set_locked(false);
        }
        RecoverResult::Wrong => ui.set_lock_message(i18n::tr("That recovery file or phrase does not work.").into()),
        RecoverResult::NotSet => ui.set_lock_message(i18n::tr("There is no recovery file for this lock.").into()),
    }
}

/// Connects the lock screen and the Security dialog. `ctl` is also what `main` uses at start.
fn wire_app_lock(ui: &MainWindow, ctl: &Rc<LockCtl>) {
    let ui_weak = ui.as_weak();

    // The helper that Slint cannot do itself: drop the last character of a code.
    ui.global::<CodeTools>().on_trim_last(|text| {
        let mut t = text.to_string();
        t.pop();
        t.into()
    });

    // ---- lock screen ----
    {
        let (ctl, weak) = (ctl.clone(), ui_weak.clone());
        ui.on_lock_submit(move |code| {
            let Some(ui) = weak.upgrade() else { return };
            let result = {
                let mut st = ctl.store.borrow_mut();
                let r = st.try_unlock(code.as_str(), applock::unix_now());
                // Every try is saved: a wrong code has to survive a crash or a restart.
                save_lock(&ui, &st);
                r
            };
            ui.set_lock_code(SharedString::default());
            match result {
                UnlockResult::Ok | UnlockResult::NotSet => {
                    ctl.tick.stop();
                    ui.set_lock_blocked(false);
                    ui.set_lock_message(SharedString::default());
                    ui.set_locked(false);
                }
                UnlockResult::Wrong { left } => {
                    ui.set_lock_message(i18n::tr_fmt("Wrong code. Tries left before a delay: {}", &[&left]).into());
                }
                UnlockResult::WrongLocked { .. } | UnlockResult::Locked { .. } => update_lock_wait(&ctl, &weak, &ui),
            }
        });
    }
    {
        let weak = ui_weak.clone();
        ui.on_lock_view_changed(move || {
            let Some(ui) = weak.upgrade() else { return };
            if !ui.get_lock_blocked() {
                ui.set_lock_message(SharedString::default());
            }
        });
    }
    {
        let (ctl, weak) = (ctl.clone(), ui_weak.clone());
        ui.on_lock_recover_text(move |text| {
            let Some(ui) = weak.upgrade() else { return };
            do_recover(&ctl, &ui, text.as_str());
        });
    }
    {
        let (ctl, weak) = (ctl.clone(), ui_weak.clone());
        ui.on_lock_recover_file(move || {
            let Some(ui) = weak.upgrade() else { return };
            let Some(path) = rfd::FileDialog::new().add_filter("Text", &["txt"]).pick_file() else { return };
            match applock::read_recovery_file(&path) {
                Some(phrase) => do_recover(&ctl, &ui, &phrase),
                None => ui.set_lock_message(i18n::tr("That recovery file or phrase does not work.").into()),
            }
        });
    }
    {
        let (ctl, weak) = (ctl.clone(), ui_weak.clone());
        ui.on_lock_now(move || {
            let Some(ui) = weak.upgrade() else { return };
            lock_now(&ctl, &weak, &ui);
        });
    }

    // ---- Settings -> Security dialog ----
    {
        let (ctl, weak) = (ctl.clone(), ui_weak.clone());
        ui.on_sec_open(move |mode| {
            let Some(ui) = weak.upgrade() else { return };
            ctl.pending.borrow_mut().take();
            ctl.new_code.borrow_mut().take();
            ui.set_sec_mode(mode);
            // The kind of the new code is chosen in the dialog; changing starts from the kind in use.
            ui.set_sec_kind(if mode == 1 { ui.get_lock_kind() } else { 0 });
            // Turning on starts with the new code; changing / turning off first asks for the current one.
            ui.set_sec_step(if mode == 0 { 1 } else { 0 });
            ui.set_sec_code(SharedString::default());
            ui.set_sec_error(SharedString::default());
            ui.set_sec_phrase(SharedString::default());
            ui.set_show_sec_popup(true);
        });
    }
    {
        let (ctl, weak) = (ctl.clone(), ui_weak.clone());
        ui.on_sec_cancel(move || {
            ctl.pending.borrow_mut().take();
            ctl.new_code.borrow_mut().take();
            if let Some(ui) = weak.upgrade() {
                ui.set_sec_phrase(SharedString::default());
                ui.set_sec_code(SharedString::default());
                ui.set_sec_error(SharedString::default());
            }
        });
    }
    {
        let (ctl, weak) = (ctl.clone(), ui_weak.clone());
        ui.on_sec_submit(move |code| {
            let Some(ui) = weak.upgrade() else { return };
            let code = code.to_string();
            let (step, mode) = (ui.get_sec_step(), ui.get_sec_mode());
            ui.set_sec_code(SharedString::default());
            match step {
                // The current code.
                0 => {
                    let result = {
                        let mut st = ctl.store.borrow_mut();
                        let r = st.try_unlock(&code, applock::unix_now());
                        save_lock(&ui, &st);
                        r
                    };
                    match result {
                        UnlockResult::Ok | UnlockResult::NotSet => {
                            ui.set_sec_error(SharedString::default());
                            if mode == 2 {
                                // Turn off: code, recovery phrase and counters are gone.
                                ctl.store.borrow_mut().clear();
                                save_lock(&ui, &ctl.store.borrow());
                                ctl.watch.stop();
                                sync_lock_flags(&ui, &ctl.store.borrow());
                                ui.set_show_sec_popup(false);
                            } else {
                                ui.set_sec_step(1);
                            }
                        }
                        UnlockResult::Wrong { left } => {
                            ui.set_sec_error(i18n::tr_fmt("Wrong code. Tries left before a delay: {}", &[&left]).into());
                        }
                        UnlockResult::WrongLocked { secs } | UnlockResult::Locked { secs } => {
                            ui.set_sec_error(i18n::tr_fmt("Too many wrong codes. Try again in {}", &[&fmt_wait(secs)]).into());
                        }
                    }
                }
                // A new code.
                1 => {
                    let kind = CodeKind::from_index(ui.get_sec_kind());
                    match applock::validate_code(kind, &code) {
                        Ok(()) => {
                            *ctl.new_code.borrow_mut() = Some(code);
                            ui.set_sec_error(SharedString::default());
                            ui.set_sec_step(2);
                        }
                        Err(e) => ui.set_sec_error(code_error_text(kind, e).into()),
                    }
                }
                // The same code again; then the recovery phrase is made (the lock is not on yet).
                2 => {
                    let first = ctl.new_code.borrow_mut().take();
                    if first.as_deref() != Some(code.as_str()) {
                        ui.set_sec_error(i18n::tr("The codes do not match. Enter a new code.").into());
                        ui.set_sec_step(1);
                        return;
                    }
                    let kind = CodeKind::from_index(ui.get_sec_kind());
                    let mut candidate = ctl.store.borrow().clone();
                    let made = candidate.set_code(kind, &code).and_then(|_| candidate.create_recovery());
                    match made {
                        Ok(phrase) => {
                            ui.set_sec_phrase(phrase.clone().into());
                            *ctl.pending.borrow_mut() = Some((candidate, phrase));
                            ui.set_sec_error(SharedString::default());
                            ui.set_sec_step(3);
                        }
                        Err(e) => {
                            ui.set_sec_error(e.into());
                            ui.set_sec_step(1);
                        }
                    }
                }
                _ => {}
            }
        });
    }
    {
        let (ctl, weak) = (ctl.clone(), ui_weak.clone());
        ui.on_sec_save_file(move || {
            let Some(ui) = weak.upgrade() else { return };
            let phrase = match ctl.pending.borrow().as_ref() {
                Some((_, p)) => p.clone(),
                None => return,
            };
            let Some(path) = rfd::FileDialog::new().set_file_name("radiant-recovery.txt").add_filter("Text", &["txt"]).save_file() else {
                return;
            };
            if let Err(e) = applock::write_recovery_file(&path, &phrase) {
                ui.set_sec_error(i18n::tr_fmt("Could not save the file: {}", &[&e]).into());
                return;
            }
            // The file is on disk: only now the new code (and its phrase) replaces the old one and the lock is on.
            let Some((candidate, _)) = ctl.pending.borrow_mut().take() else { return };
            *ctl.store.borrow_mut() = candidate;
            save_lock(&ui, &ctl.store.borrow());
            sync_lock_flags(&ui, &ctl.store.borrow());
            ui.set_sec_phrase(SharedString::default());
            ui.set_sec_error(SharedString::default());
            ui.set_show_sec_popup(false);
            start_lock_watch(&ctl, &weak);
        });
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

        // The same for the open queue page.
        let q_index = status
            .current
            .as_ref()
            .and_then(|c| Q_ROWS.lock().ok()?.iter().position(|p| p == c))
            .map_or(-1, |i| i as i32);

        // The same for the open playlist page.
        let pl_index = status
            .current
            .as_ref()
            .and_then(|c| PL_ROWS.lock().ok()?.iter().position(|p| p == c))
            .map_or(-1, |i| i as i32);

        // A new track started: the big player view needs its details (looked up only on a change).
        let mut changed_path: Option<PathBuf> = None;
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
                    changed_path = Some(current.clone());
                    now_library.lock().ok().and_then(|l| l.iter().find(|t| &t.path == current).cloned())
                } else {
                    None
                }
            }
            None => None,
        };
        // Current line of the synchronized lyrics (binary search over the start times).
        // A track that just changed starts without a highlighted line (the old times are about to be cleared).
        let lyric_index = if changed_path.is_some() {
            -1
        } else {
            LYRIC_TIMES
                .lock()
                .ok()
                .map_or(-1, |times| radiant_core::lyrics::current_line(&times, status.position_ms))
        };

        let _ = weak.upgrade_in_event_loop(move |ui| {
            if let Some(track) = &changed_track {
                show_now_playing(&ui, track);
            }
            // The equalizer follows the track (per-track mode); handled on the UI thread by Rust.
            if let Some(path) = &changed_path {
                let path_text: String = path.to_string_lossy().into_owned();
                ui.invoke_now_track_changed(path_text.into());
            }
            ui.set_lyric_index(lyric_index);
            ui.set_current_index(index);
            ui.set_album_current_index(album_index);
            ui.set_pl_current_index(pl_index);
            ui.set_queue_current_index(q_index);
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

    // Language first, so that every text made below is already translated. Until the settings are read
    // (and when they cannot be) the language of Windows is used.
    apply_language(settings::LANGUAGE_SYSTEM);
    let settings = match settings::load() {
        Ok(s) => Rc::new(RefCell::new(s)),
        Err(e) => {
            ui.set_status(i18n::tr_fmt("Could not read settings: {}", &[&e]).into());
            Rc::new(RefCell::new(AppSettings::default()))
        }
    };
    apply_language(&settings.borrow().language);
    ui.set_language(i18n::setting_to_index(&settings.borrow().language));
    ui.set_stem_gpu(settings.borrow().stems_gpu);
    let pl: PlRef = {
        let store = match playlists::load() {
            Ok(store) => store,
            Err(e) => {
                ui.set_status(i18n::tr_fmt("Could not read playlists: {}", &[&e]).into());
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
    let queues_state: QRef = {
        let store = match queues::load() {
            Ok(store) => store,
            Err(e) => {
                ui.set_status(i18n::tr_fmt("Could not read queues: {}", &[&e]).into());
                let mut store = QueueStore::default();
                store.normalize();
                store
            }
        };
        Rc::new(RefCell::new(QState { store, target: None }))
    };
    ui.set_queue_index(settings.borrow().queue_index as i32);
    refresh_queues(&ui, &queues_state.borrow());
    fill_queue_page(&ui, &queues_state.borrow(), &library);
    let ui_weak = ui.as_weak();
    // Bumped on every folder scan so a late result cannot replace a newer one.
    let scan_gen: Arc<AtomicU64> = Arc::new(AtomicU64::new(0));

    // ---- sound settings (panel in the UI -> engine) ----
    // Rust owns the current settings; every change sends the whole set to the engine.
    let fx = Rc::new(RefCell::new(AudioFxSettings::default()));
    let eq_model: Rc<VecModel<f32>> = Rc::new(VecModel::from(vec![0.0_f32; 12]));
    ui.set_eq_gains(ModelRc::from(eq_model.clone()));
    // Track whose curve is shown in the per-track mode (the one that started last).
    let eq_track: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    apply_settings(&ui, &player, &fx, &eq_model, &settings.borrow());
    if let Ok(mut order) = MANUAL_ORDER.lock() { *order = settings.borrow().manual_order.clone(); }
    if let Ok(mut s) = ALBUM_SORT.lock() { *s = (settings.borrow().album_sort, settings.borrow().album_sort_desc); }
    if let Ok(mut order) = ALBUM_ORDER.lock() { *order = settings.borrow().album_order.clone(); }
    if let Some(dir) = settings.borrow().music_folder.clone() {
        ui.set_music_folder(dir.display().to_string().into());
        scan_folder(ui_weak.clone(), library.clone(), displayed.clone(), query.clone(), sort.clone(), scan_gen.clone(), dir, false);
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
    let stems_ctl = StemsCtl::new();
    {
        // Context menu "Create stems": opens the window where the parts are chosen (the same switches as on the Stems
        // tab). Files go to `<music folder>/Stems/<Artist - Title>/`. Never in low-power mode; needs the model.
        let (weak, library, displayed) = (ui_weak.clone(), library.clone(), displayed.clone());
        ui.on_create_stems(move |row, in_album| {
            let row = row.max(0) as usize;
            let path = if in_album {
                OPEN_ROWS.lock().ok().and_then(|r| r.get(row).cloned()).flatten()
            } else {
                displayed.lock().ok().and_then(|l| l.get(row).cloned())
            };
            let Some(path) = path else { return };
            let Some(ui) = weak.upgrade() else { return };
            // A stem made by the program is not separated again: no window, just the reason in the status line.
            if radiant_stems::job::is_stem_file(&path) {
                ui.set_status(i18n::tr("Stems cannot be made from stems").into());
                return;
            }
            let (artist, title) = library
                .lock()
                .ok()
                .and_then(|l| l.iter().find(|t| t.path == path).map(|t| (t.artist.clone(), t.title.clone())))
                .unwrap_or_default();
            let base = stems_base(&path, artist.as_deref(), &title);
            if let Ok(mut t) = STEMS_MENU_TARGET.lock() {
                *t = Some((path, base.clone()));
            }
            ui.set_stems_popup_title(base.into());
            if !ui.get_stems_busy() {
                ui.set_stems_info("".into());
            }
            ui.set_show_stems_popup(true);
            update_stems_file_info(&ui);
        });
    }
    {
        // "Create stems" in the window: starts the job for the remembered track; the window closes when it started.
        let (settings, weak, ctl) = (settings.clone(), ui_weak.clone(), stems_ctl.clone());
        ui.on_stems_popup_create(move || {
            let Some(ui) = weak.upgrade() else { return };
            let (selection, each_part) = stems_choice(&ui);
            let Some((path, base)) = STEMS_MENU_TARGET.lock().ok().and_then(|t| t.clone()) else { return };
            if ctl.busy.load(Ordering::SeqCst) {
                ui.set_stems_info(i18n::tr("Another separation is already running").into());
                return;
            }
            begin_stems(&weak, &settings, &ctl, path, base, radiant_stems::output_sets(selection, each_part));
            if ctl.busy.load(Ordering::SeqCst) {
                ui.set_show_stems_popup(false);
                update_stems_file_info(&ui);
            }
        });
    }
    {
        // A finished job: its files go into a playlist named like the track, inside the Stems folder; the folder is
        // rescanned so the new files are in the library (a playlist shows files that are not in it as missing).
        let (weak, pl, settings) = (ui_weak.clone(), pl.clone(), settings.clone());
        ui.on_stems_finished(move || {
            let Some(ui) = weak.upgrade() else { return };
            let done: Vec<(String, Vec<PathBuf>)> =
                STEMS_DONE.lock().map(|mut d| std::mem::take(&mut *d)).unwrap_or_default();
            if done.is_empty() {
                return;
            }
            {
                let mut st = pl.borrow_mut();
                for (name, files) in &done {
                    st.store.add_stems_playlist(name, files);
                }
                save_playlists(&ui, &st.store);
                refresh_playlists(&ui, &mut st);
            }
            if settings.borrow().music_folder.is_some() {
                ui.invoke_rescan_folder();
            }
        });
    }
    {
        // Stems tab of the player: the switches change the line with the file name, "Create" starts the job for the
        // playing track, "Cancel" stops it (the half-written file is never left behind, see `wav.rs`).
        let (settings, weak, ctl) = (settings.clone(), ui_weak.clone(), stems_ctl.clone());
        ui.on_stems_create(move || {
            let Some(ui) = weak.upgrade() else { return };
            let (selection, each_part) = stems_choice(&ui);
            let Some((path, base)) = NOW_STEMS_TRACK.lock().ok().and_then(|t| t.clone()) else { return };
            begin_stems(&weak, &settings, &ctl, path, base, radiant_stems::output_sets(selection, each_part));
        });
    }
    {
        let weak = ui_weak.clone();
        ui.on_stems_pick_changed(move || {
            if let Some(ui) = weak.upgrade() {
                update_stems_file_info(&ui);
            }
        });
    }
    {
        let (weak, ctl) = (ui_weak.clone(), stems_ctl.clone());
        ui.on_stems_cancel(move || {
            if ctl.busy.load(Ordering::SeqCst) {
                ctl.cancel.store(true, Ordering::SeqCst);
                if let Some(ui) = weak.upgrade() {
                    ui.set_stems_info(i18n::tr("Cancelling…").into());
                }
            }
        });
    }
    {
        // Settings -> AI & stems -> graphics card switch.
        let (settings, weak) = (settings.clone(), ui_weak.clone());
        ui.on_stem_gpu_changed(move |on| {
            settings.borrow_mut().stems_gpu = on;
            save_settings(&weak, &settings);
        });
    }
    {
        // Sort panel of the Albums grid: applied at once and remembered.
        let (weak, library, query, settings) = (ui_weak.clone(), library.clone(), query.clone(), settings.clone());
        ui.on_album_sort_changed(move |mode, descending| {
            let mode = mode.clamp(0, 3) as u8;
            let descending = descending && mode != 0;
            if let Ok(mut s) = ALBUM_SORT.lock() { *s = (mode, descending); }
            {
                let mut cfg = settings.borrow_mut();
                cfg.album_sort = mode;
                cfg.album_sort_desc = descending;
            }
            save_settings(&weak, &settings);
            refresh_albums(&weak, &library, &query);
        });
    }
    {
        // Drag of an album tile: the grid on screen becomes the manual order, with the tile moved.
        let (weak, library, query, settings) = (ui_weak.clone(), library.clone(), query.clone(), settings.clone());
        ui.on_album_move(move |from, to| {
            if from < 0 || to < 0 || query.lock().map(|q| !q.trim().is_empty()).unwrap_or(true) {
                return;
            }
            let mut order: Vec<String> = ALBUMS.lock().map(|a| a.iter().map(|e| e.key.clone()).collect()).unwrap_or_default();
            let (from, to) = (from as usize, to as usize);
            if from >= order.len() || to >= order.len() || from == to {
                return;
            }
            let moved = order.remove(from);
            order.insert(to, moved);
            if let Ok(mut o) = ALBUM_ORDER.lock() { *o = order.clone(); }
            if let Ok(mut s) = ALBUM_SORT.lock() { *s = (0, false); }
            {
                let mut cfg = settings.borrow_mut();
                cfg.album_order = order;
                cfg.album_sort = 0;
                cfg.album_sort_desc = false;
            }
            save_settings(&weak, &settings);
            if let Some(ui) = weak.upgrade() {
                ui.set_album_sort_mode(0);
                ui.set_album_sort_descending(false);
            }
            refresh_albums(&weak, &library, &query);
        });
    }
    {
        // Drag of a playlist on the Playlists page: the order on screen (inside its group) becomes the manual order.
        let (weak, pl, settings) = (ui_weak.clone(), pl.clone(), settings.clone());
        ui.on_pl_move_playlist(move |from, to| {
            let Some(ui) = weak.upgrade() else { return };
            if from < 0 || to < 0 || from == to {
                return;
            }
            let mut st = pl.borrow_mut();
            let (Some(ListEntry::Playlist(a)), Some(ListEntry::Playlist(b))) =
                (st.rows.get(from as usize).copied(), st.rows.get(to as usize).copied())
            else { return };
            let (from_id, to_id, folder) = {
                let (pa, pb) = (&st.store.playlists[a], &st.store.playlists[b]);
                if pa.id == FAVORITES_ID || pb.id == FAVORITES_ID || pa.folder_id != pb.folder_id {
                    return;
                }
                (pa.id.clone(), pb.id.clone(), pa.folder_id.clone())
            };
            // The group as shown now (same folder, Liked tracks left out).
            let mut ids: Vec<String> = st
                .rows
                .iter()
                .filter_map(|e| match *e {
                    ListEntry::Playlist(i) => {
                        let p = &st.store.playlists[i];
                        (p.id != FAVORITES_ID && p.folder_id == folder).then(|| p.id.clone())
                    }
                    ListEntry::Folder(_) => None,
                })
                .collect();
            let (Some(fp), Some(tp)) = (ids.iter().position(|i| *i == from_id), ids.iter().position(|i| *i == to_id)) else { return };
            let moved = ids.remove(fp);
            ids.insert(tp, moved);
            if !st.store.set_playlist_order(&ids) && st.sort.key == PlaylistSortKey::Manual {
                return;
            }
            st.sort = PlaylistSort { key: PlaylistSortKey::Manual, descending: false };
            save_playlists(&ui, &st.store);
            refresh_playlist_rows(&ui, &mut st);
            ui.set_pl_sort_mode(4);
            ui.set_pl_sort_descending(false);
            {
                let mut cfg = settings.borrow_mut();
                cfg.playlist_sort = 4;
                cfg.playlist_sort_desc = false;
            }
            save_settings(&weak, &settings);
        });
    }
    {
        // Sort panel of the Playlists page: the order is applied at once and remembered.
        let (weak, pl, settings) = (ui_weak.clone(), pl.clone(), settings.clone());
        ui.on_pl_sort_changed(move |mode, descending| {
            let Some(ui) = weak.upgrade() else { return };
            let mode = mode.clamp(0, 4) as u8;
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
        // Context menu "Add to queue" (right click or three dots): window with the four queues.
        let (weak, qs, library, displayed) = (ui_weak.clone(), queues_state.clone(), library.clone(), displayed.clone());
        ui.on_add_to_queue(move |row, in_album| {
            let row = row.max(0) as usize;
            let path = if in_album {
                OPEN_ROWS.lock().ok().and_then(|r| r.get(row).cloned()).flatten()
            } else {
                displayed.lock().ok().and_then(|l| l.get(row).cloned())
            };
            let (Some(path), Some(ui)) = (path, weak.upgrade()) else { return };
            open_queue_window(&ui, &qs, &library, path);
        });
    }
    {
        // The same menu entry on a track of the open playlist page.
        let (weak, qs, library) = (ui_weak.clone(), queues_state.clone(), library.clone());
        ui.on_pl_page_add_to_queue(move |row| {
            let path = PL_ROWS.lock().ok().and_then(|r| r.get(row.max(0) as usize).cloned());
            let (Some(path), Some(ui)) = (path, weak.upgrade()) else { return };
            open_queue_window(&ui, &qs, &library, path);
        });
    }
    {
        // Button next to the volume: the track that is playing now.
        let (weak, qs, library) = (ui_weak.clone(), queues_state.clone(), library.clone());
        ui.on_add_current_to_queue(move || {
            let path = LAST_NOW.lock().ok().and_then(|l| l.clone());
            let (Some(path), Some(ui)) = (path, weak.upgrade()) else { return };
            open_queue_window(&ui, &qs, &library, path);
        });
    }
    {
        // A queue row was clicked: add the track to that queue, or take it out again.
        let (weak, qs, library) = (ui_weak.clone(), queues_state.clone(), library.clone());
        ui.on_queue_toggle(move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = qs.borrow_mut();
            let i = i.max(0) as usize;
            let Some(target) = st.target.clone() else { return };
            let changed = if st.store.contains(i, &target) {
                st.store.remove(i, &target)
            } else {
                st.store.add(i, &target)
            };
            if !changed { return; }
            save_queues(&ui, &st.store);
            refresh_queues(&ui, &st);
            fill_queue_page(&ui, &st, &library);
        });
    }
    {
        // Queue page: menu entries "Add to playlist" / "Add to queue" / "Open album" for its tracks.
        let (weak, pl, library) = (ui_weak.clone(), pl.clone(), library.clone());
        ui.on_queue_page_add_to_playlist(move |row| {
            let path = Q_ROWS.lock().ok().and_then(|r| r.get(row.max(0) as usize).cloned());
            let (Some(path), Some(ui)) = (path, weak.upgrade()) else { return };
            open_add_window(&ui, &pl, &library, path);
        });
    }
    {
        let (weak, qs, library) = (ui_weak.clone(), queues_state.clone(), library.clone());
        ui.on_queue_page_add_to_queue(move |row| {
            let path = Q_ROWS.lock().ok().and_then(|r| r.get(row.max(0) as usize).cloned());
            let (Some(path), Some(ui)) = (path, weak.upgrade()) else { return };
            open_queue_window(&ui, &qs, &library, path);
        });
    }
    {
        let weak = ui_weak.clone();
        ui.on_album_index_of_queue_track(move |i| {
            let Some(path) = Q_ROWS.lock().ok().and_then(|r| r.get(i.max(0) as usize).cloned()) else { return -1 };
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
        // Click on a track of the open queue: it is played from that track on.
        let (weak, qs, player, library) = (ui_weak.clone(), queues_state.clone(), player.clone(), library.clone());
        ui.on_queue_play_track(move |i| {
            let Some(ui) = weak.upgrade() else { return };
            let Some(clicked) = Q_ROWS.lock().ok().and_then(|r| r.get(i.max(0) as usize).cloned()) else { return };
            let queue = queue_playable(&qs.borrow(), &ui, &library);
            match queue.iter().position(|p| *p == clicked) {
                Some(start) => player.send(Command::SetQueue { queue, start }),
                None => ui.set_status(i18n::tr("This file is not in the library").into()),
            }
        });
    }
    {
        let (weak, qs, player, library) = (ui_weak.clone(), queues_state.clone(), player.clone(), library.clone());
        ui.on_queue_play_all(move || {
            let Some(ui) = weak.upgrade() else { return };
            let queue = queue_playable(&qs.borrow(), &ui, &library);
            if queue.is_empty() {
                ui.set_status(i18n::tr("This queue has no playable tracks yet").into());
                return;
            }
            player.send(Command::SetQueue { queue, start: 0 });
        });
    }
    {
        // "Shuffle" on the queue page: shuffle on, start from a random track.
        let (weak, qs, player, library, settings) = (ui_weak.clone(), queues_state.clone(), player.clone(), library.clone(), settings.clone());
        ui.on_queue_shuffle_all(move || {
            let Some(ui) = weak.upgrade() else { return };
            let queue = queue_playable(&qs.borrow(), &ui, &library);
            if queue.is_empty() {
                ui.set_status(i18n::tr("This queue has no playable tracks yet").into());
                return;
            }
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos() as usize)
                .unwrap_or(0);
            let start = nanos % queue.len();
            player.send(Command::SetShuffle(true));
            settings.borrow_mut().shuffle = true;
            ui.set_shuffle(true);
            save_settings(&weak, &settings);
            player.send(Command::SetQueue { queue, start });
        });
    }
    {
        // Drag on a row of the open queue: the new order is saved.
        let (weak, qs, library) = (ui_weak.clone(), queues_state.clone(), library.clone());
        ui.on_queue_move_track(move |from, to| {
            let Some(ui) = weak.upgrade() else { return };
            if from < 0 || to < 0 { return; }
            let mut st = qs.borrow_mut();
            let q = ui.get_queue_index().clamp(0, 3) as usize;
            if st.store.move_track(q, from as usize, to as usize) {
                save_queues(&ui, &st.store);
                fill_queue_page(&ui, &st, &library);
            }
        });
    }
    {
        // "Remove from queue" in the track menu.
        let (weak, qs, library) = (ui_weak.clone(), queues_state.clone(), library.clone());
        ui.on_queue_remove(move |row| {
            let Some(ui) = weak.upgrade() else { return };
            let Some(path) = Q_ROWS.lock().ok().and_then(|r| r.get(row.max(0) as usize).cloned()) else { return };
            let mut st = qs.borrow_mut();
            let q = ui.get_queue_index().clamp(0, 3) as usize;
            if st.store.remove(q, &path) {
                save_queues(&ui, &st.store);
                refresh_queues(&ui, &st);
                fill_queue_page(&ui, &st, &library);
            }
        });
    }
    {
        // "Clear" button of the queue page: asks first (the delete window is reused; its button calls `queue-clear`).
        let (weak, pl, qs) = (ui_weak.clone(), pl.clone(), queues_state.clone());
        ui.on_queue_clear_ask(move || {
            let Some(ui) = weak.upgrade() else { return };
            let q = ui.get_queue_index().clamp(0, 3) as usize;
            if qs.borrow().store.tracks(q).is_empty() { return; }
            let mut st = pl.borrow_mut();
            st.target = None;
            st.moving = None;
            st.edit = None;
            ui.set_pl_dialog_title(i18n::tr_fmt("Clear queue {}?", &[&(q + 1)]).into());
            ui.set_pl_dialog_text(i18n::tr("All tracks will be removed from this queue. The files stay in your library.").into());
            show_pl_popup(&ui, &mut st, 4);
        });
    }
    {
        // Confirmed "Clear" on the queue page.
        let (weak, qs, library) = (ui_weak.clone(), queues_state.clone(), library.clone());
        ui.on_queue_clear(move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut st = qs.borrow_mut();
            let q = ui.get_queue_index().clamp(0, 3) as usize;
            if st.store.clear(q) {
                save_queues(&ui, &st.store);
                refresh_queues(&ui, &st);
                fill_queue_page(&ui, &st, &library);
            }
        });
    }
    {
        // Another queue was opened, or the library was rescanned.
        let (weak, qs, library) = (ui_weak.clone(), queues_state.clone(), library.clone());
        ui.on_queue_page_refresh(move || {
            let Some(ui) = weak.upgrade() else { return };
            fill_queue_page(&ui, &qs.borrow(), &library);
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
                    if let Some(ui) = weak.upgrade() { ui.set_status(i18n::tr("This file is not in the library").into()); }
                }
            }
        });
    }
    {
        let (weak, pl, player, library) = (ui_weak.clone(), pl.clone(), player.clone(), library.clone());
        ui.on_pl_play_all(move || {
            let queue = playable_queue(&pl.borrow(), &library);
            if queue.is_empty() {
                if let Some(ui) = weak.upgrade() { ui.set_status(i18n::tr("This playlist has no playable tracks yet").into()); }
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
                if let Some(ui) = weak.upgrade() { ui.set_status(i18n::tr("This playlist has no playable tracks yet").into()); }
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
            ui.set_pl_dialog_title(i18n::tr("Rename playlist").into());
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
            ui.set_pl_dialog_title(i18n::tr("Delete playlist?").into());
            ui.set_pl_dialog_text(i18n::tr_fmt("\"{}\" will be deleted. The tracks stay in your library.", &[&name]).into());
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
            // The Stems folder cannot be renamed or deleted (the core refuses too; the buttons are hidden).
            if PlaylistStore::folder_is_locked(&id) {
                return;
            }
            st.target = None;
            st.moving = None;
            st.edit = Some(Edit::Folder(id));
            ui.set_pl_dialog_title(i18n::tr("Rename folder").into());
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
            // The Stems folder cannot be renamed or deleted (the core refuses too; the buttons are hidden).
            if PlaylistStore::folder_is_locked(&id) {
                return;
            }
            st.target = None;
            st.moving = None;
            st.edit = Some(Edit::Folder(id));
            ui.set_pl_dialog_title(i18n::tr("Delete folder?").into());
            ui.set_pl_dialog_text(i18n::tr_fmt("The folder \"{}\" will be deleted. Its playlists are kept and move out of the folder.", &[&name]).into());
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
        // Open section of the settings page: remembered between launches.
        let (settings, weak) = (settings.clone(), ui_weak.clone());
        ui.on_settings_section_changed(move |i| {
            settings.borrow_mut().settings_section = i.clamp(0, 5) as u8;
            save_settings(&weak, &settings);
        });
    }
    {
        // Palette choice per theme kind: the UI already switched itself, here it is only remembered.
        let (settings, weak) = (settings.clone(), ui_weak.clone());
        ui.on_palette_changed(move |light, dark, black| {
            settings.borrow_mut().palettes = [light.clamp(0, 10) as u8, dark.clamp(0, 10) as u8, black.clamp(0, 10) as u8];
            save_settings(&weak, &settings);
        });
    }
    {
        // Save the current own color as a preset (no duplicates, at most MAX_SAVED_COLORS).
        let (settings, weak) = (settings.clone(), ui_weak.clone());
        ui.on_custom_save(move || {
            {
                let mut st = settings.borrow_mut();
                let cur = st.custom_hsv;
                let rgb = |c: &[f32; 3]| radiant_core::theme::hsv_to_rgb(c[0], c[1], c[2]);
                let exists = st.saved_colors.iter().any(|c| rgb(c) == rgb(&cur));
                if !exists && st.saved_colors.len() < settings::MAX_SAVED_COLORS {
                    st.saved_colors.push(cur);
                }
            }
            if let Some(ui) = weak.upgrade() {
                refresh_saved_colors(&ui, &settings.borrow().saved_colors);
            }
            save_settings(&weak, &settings);
        });
    }
    {
        // Click on a saved own color: it becomes the current own color.
        let (settings, weak) = (settings.clone(), ui_weak.clone());
        ui.on_custom_pick(move |i| {
            let hsv = settings.borrow().saved_colors.get(i.max(0) as usize).copied();
            if let Some(hsv) = hsv {
                settings.borrow_mut().custom_hsv = hsv;
                if let Some(ui) = weak.upgrade() {
                    apply_custom_color(&ui, hsv);
                }
                save_settings(&weak, &settings);
            }
        });
    }
    {
        // Right click on a saved own color: remove it.
        let (settings, weak) = (settings.clone(), ui_weak.clone());
        ui.on_custom_remove(move |i| {
            {
                let mut st = settings.borrow_mut();
                let i = i.max(0) as usize;
                if i < st.saved_colors.len() {
                    st.saved_colors.remove(i);
                }
            }
            if let Some(ui) = weak.upgrade() {
                refresh_saved_colors(&ui, &settings.borrow().saved_colors);
            }
            save_settings(&weak, &settings);
        });
    }
    {
        // Own accent color (HSV sliders in the settings).
        let (settings, weak) = (settings.clone(), ui_weak.clone());
        ui.on_custom_color_edited(move |h, s, v| {
            let hsv = [h.clamp(0.0, 1.0), s.clamp(0.0, 1.0), v.clamp(0.0, 1.0)];
            settings.borrow_mut().custom_hsv = hsv;
            if let Some(ui) = weak.upgrade() {
                apply_custom_color(&ui, hsv);
            }
            save_settings(&weak, &settings);
        });
    }
    {
        // Interface language (System + 10): switches on the fly; playback is not touched. `@tr` texts in the
        // .slint file follow by themselves, the texts made in Rust are rebuilt here.
        let (settings, weak, library, query, pl, qs) =
            (settings.clone(), ui_weak.clone(), library.clone(), query.clone(), pl.clone(), queues_state.clone());
        ui.on_language_changed(move |index| {
            let setting = i18n::index_to_setting(index);
            settings.borrow_mut().language = setting.to_string();
            save_settings(&weak, &settings);
            apply_language(setting);
            let Some(ui) = weak.upgrade() else { return };
            ui.set_language(index.clamp(0, 10));
            ui.set_status(SharedString::default());
            refresh_translated_texts(&ui);
            refresh_albums(&weak, &library, &query);
            refresh_playlists(&ui, &mut pl.borrow_mut());
            refresh_queues(&ui, &qs.borrow());
            fill_queue_page(&ui, &qs.borrow(), &library);
        });
    }
    {
        // Theme (System / Light / Dark / Black): the UI already switched itself, here it is only remembered.
        let (settings, weak) = (settings.clone(), ui_weak.clone());
        ui.on_theme_changed(move |mode| {
            settings.borrow_mut().theme = mode.clamp(0, 3) as u8;
            save_settings(&weak, &settings);
        });
    }
    // Low-power mode. The switch in the settings (or the chip in the Sound panel) has already flipped `Theme.low-power`
    // (animations stop at once); here the choice is remembered, the engine bypasses its effects and the rays are
    // frozen or restarted. The user's own sound settings stay stored and come back when the mode is turned off.
    let rays_timer = Rc::new(slint::Timer::default());
    let low_power_start = settings.borrow().low_power;
    {
        let (settings, weak, player, timer) = (settings.clone(), ui_weak.clone(), player.clone(), rays_timer.clone());
        ui.on_low_power_changed(move |on| {
            settings.borrow_mut().low_power = on;
            save_settings(&weak, &settings);
            player.send(Command::SetLowPower(on));
            apply_low_power_visuals(&weak, &timer, &player, on);
        });
    }
    {
        // AI & stems -> Demucs model card: download / import / cancel / delete. Not in low-power mode (heavy work).
        let (cancel, busy) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
        {
            let (weak, settings, cancel, busy) = (ui_weak.clone(), settings.clone(), cancel.clone(), busy.clone());
            ui.on_stem_download(move || {
                if !radiant_stems::allowed(settings.borrow().low_power) {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_status(i18n::tr("Stems are not available in low-power mode").into());
                    }
                    return;
                }
                run_stem_job(
                    &weak,
                    &cancel,
                    &busy,
                    "The file did not match its checksum and was removed",
                    |store, cancel, progress| store.download(cancel, progress),
                );
            });
        }
        {
            let (weak, settings, cancel, busy) = (ui_weak.clone(), settings.clone(), cancel.clone(), busy.clone());
            ui.on_stem_import(move || {
                if !radiant_stems::allowed(settings.borrow().low_power) {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_status(i18n::tr("Stems are not available in low-power mode").into());
                    }
                    return;
                }
                let Some(path) = rfd::FileDialog::new().add_filter("ONNX model", &["onnx"]).pick_file() else { return };
                run_stem_job(
                    &weak,
                    &cancel,
                    &busy,
                    "This file is not the expected model (checksum mismatch)",
                    move |store, cancel, progress| store.import(&path, cancel, progress),
                );
            });
        }
        {
            let cancel = cancel.clone();
            ui.on_stem_cancel(move || cancel.store(true, Ordering::SeqCst));
        }
        {
            let (weak, busy) = (ui_weak.clone(), busy.clone());
            ui.on_stem_delete(move || {
                if busy.load(Ordering::SeqCst) {
                    return;
                }
                let message = match stem_store().delete() {
                    Ok(()) => i18n::tr("The model was deleted"),
                    Err(e) => i18n::tr_fmt("Could not delete the model: {}", &[&e]),
                };
                if let Some(ui) = weak.upgrade() {
                    show_stem_state(&ui);
                    ui.set_status(message.into());
                }
            });
        }
    }
    {
        // Section / queue switch: remembered between launches.
        let (settings, weak, qs, library) = (settings.clone(), ui_weak.clone(), queues_state.clone(), library.clone());
        ui.on_section_changed(move |section, queue| {
            {
                let mut st = settings.borrow_mut();
                st.section = section.clamp(0, 3) as u8;
                st.queue_index = queue.clamp(0, 3) as u8;
            }
            save_settings(&weak, &settings);
            if section == 3 {
                if let Some(ui) = weak.upgrade() { fill_queue_page(&ui, &qs.borrow(), &library); }
            }
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
        let (player, settings, eq_track, weak) =
            (player.clone(), settings.clone(), eq_track.clone(), ui_weak.clone());
        ui.on_speed_changed(move |t| {
            let t = (t * 10.0).round() / 10.0;
            // Slider position -2..=2 -> speed 1/3..3 (same mapping as Android).
            player.send(Command::SetSpeed(slider_to_speed(t)));
            // Per-track mode stores it for the current track, otherwise it is global.
            let track = eq_track.borrow().clone();
            settings.borrow_mut().set_speed_for_track(track.as_deref(), t);
            save_settings(&weak, &settings);
        });
    }

    {
        let (player, fx, settings, eq_track, weak) =
            (player.clone(), fx.clone(), settings.clone(), eq_track.clone(), ui_weak.clone());
        ui.on_normalize_changed(move |on| {
            let mut s = fx.borrow_mut();
            s.normalize = on;
            player.send(Command::SetFx(s.clone()));
            let track = eq_track.borrow().clone();
            settings.borrow_mut().set_fx_for_track(track.as_deref(), &s);
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, fx, settings, eq_track, weak) =
            (player.clone(), fx.clone(), settings.clone(), eq_track.clone(), ui_weak.clone());
        ui.on_mono_changed(move |on| {
            let mut s = fx.borrow_mut();
            s.mono = on;
            player.send(Command::SetFx(s.clone()));
            let track = eq_track.borrow().clone();
            settings.borrow_mut().set_fx_for_track(track.as_deref(), &s);
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, fx, settings, eq_track, weak) =
            (player.clone(), fx.clone(), settings.clone(), eq_track.clone(), ui_weak.clone());
        ui.on_bass_changed(move |v| {
            let mut s = fx.borrow_mut();
            s.bass_db = (v * 2.0).round() / 2.0; // 0.5 dB steps
            player.send(Command::SetFx(s.clone()));
            let track = eq_track.borrow().clone();
            settings.borrow_mut().set_fx_for_track(track.as_deref(), &s);
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, fx, settings, eq_track, weak) =
            (player.clone(), fx.clone(), settings.clone(), eq_track.clone(), ui_weak.clone());
        ui.on_treble_changed(move |v| {
            let mut s = fx.borrow_mut();
            s.treble_db = (v * 2.0).round() / 2.0;
            player.send(Command::SetFx(s.clone()));
            let track = eq_track.borrow().clone();
            settings.borrow_mut().set_fx_for_track(track.as_deref(), &s);
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, fx, settings, eq_track, weak) =
            (player.clone(), fx.clone(), settings.clone(), eq_track.clone(), ui_weak.clone());
        ui.on_subsonic_changed(move |v| {
            let mut s = fx.borrow_mut();
            s.subsonic_percent = v.round(); // whole percent
            player.send(Command::SetFx(s.clone()));
            let track = eq_track.borrow().clone();
            settings.borrow_mut().set_fx_for_track(track.as_deref(), &s);
            save_settings(&weak, &settings);
        });
    }
    {
        let (player, fx, eq_model, settings, eq_track, weak) =
            (player.clone(), fx.clone(), eq_model.clone(), settings.clone(), eq_track.clone(), ui_weak.clone());
        ui.on_eq_changed(move |i, v| {
            let i = i as usize;
            if i >= 12 {
                return;
            }
            let v = (v * 2.0).round() / 2.0;
            let track = eq_track.borrow().clone();
            let mut gains = settings.borrow().effective_eq(track.as_deref());
            gains[i] = v;
            apply_curve(&weak, &player, &fx, &eq_model, &settings, track.as_deref(), gains);
        });
    }
    // Preset buttons: names for the UI, and applying a curve replaces all 12 gains at once.
    refresh_translated_texts(&ui);
    let (preset_lines, preset_fills): (Vec<SharedString>, Vec<SharedString>) = EQ_PRESETS
        .iter()
        .map(|p| {
            let (line, fill) = eq_curve::mini_paths(&p.gains);
            (SharedString::from(line), SharedString::from(fill))
        })
        .unzip();
    ui.set_eq_preset_lines(ModelRc::new(VecModel::from(preset_lines)));
    ui.set_eq_preset_fills(ModelRc::new(VecModel::from(preset_fills)));
    {
        let (player, fx, eq_model, settings, eq_track, weak) =
            (player.clone(), fx.clone(), eq_model.clone(), settings.clone(), eq_track.clone(), ui_weak.clone());
        ui.on_eq_preset_selected(move |i| {
            let Some(preset) = EQ_PRESETS.get(i as usize) else { return };
            let track = eq_track.borrow().clone();
            {
                // A preset works in every mode: with the equalizer off it turns it on (one curve for all tracks).
                let mut s = settings.borrow_mut();
                if s.eq_mode == EqMode::Off {
                    s.eq_mode = EqMode::Global;
                }
                // Per-track mode needs a track to give the curve to.
                if s.eq_mode == EqMode::PerTrack && track.is_none() {
                    return;
                }
            }
            apply_curve(&weak, &player, &fx, &eq_model, &settings, track.as_deref(), preset.gains);
        });
    }
    // Equalizer mode: off / one curve for all tracks / own curve per track.
    {
        let (player, fx, eq_model, settings, eq_track, weak) =
            (player.clone(), fx.clone(), eq_model.clone(), settings.clone(), eq_track.clone(), ui_weak.clone());
        ui.on_eq_mode_changed(move |k| {
            settings.borrow_mut().eq_mode = EqMode::from_index(k);
            if let Some(ui) = weak.upgrade() {
                let track = eq_track.borrow().clone();
                refresh_eq(&ui, &player, &fx, &eq_model, &settings.borrow(), track.as_deref());
            }
            save_settings(&weak, &settings);
        });
    }
    // Another track started (reported by the status timer): per-track mode loads its curve.
    {
        let (player, fx, eq_model, settings, eq_track, weak) =
            (player.clone(), fx.clone(), eq_model.clone(), settings.clone(), eq_track.clone(), ui_weak.clone());
        ui.on_now_track_changed(move |path| {
            let track = PathBuf::from(path.as_str());
            *eq_track.borrow_mut() = Some(track.clone());
            if let Some(ui) = weak.upgrade() {
                refresh_eq(&ui, &player, &fx, &eq_model, &settings.borrow(), Some(&track));
            }
        });
    }
    {
        let (player, fx, eq_model, settings, eq_track) =
            (player.clone(), fx.clone(), eq_model.clone(), settings.clone(), eq_track.clone());
        let weak = ui.as_weak();
        ui.on_fx_reset(move || {
            let track = eq_track.borrow().clone();
            settings.borrow_mut().reset_sound(track.as_deref());
            if let Some(ui) = weak.upgrade() {
                ui.set_fx_normalize(false);
                ui.set_fx_mono(false);
                // Bass, treble and subsonic are shown by refresh_eq (the speed is not reset).
                refresh_eq(&ui, &player, &fx, &eq_model, &settings.borrow(), track.as_deref());
            }
            save_settings(&weak, &settings);
        });
    }

    // Manual rescan of the remembered folder (the folder is also rescanned on every start).
    {
        let (weak, settings, library, displayed, query, sort, scan_gen) =
            (ui_weak.clone(), settings.clone(), library.clone(), displayed.clone(), query.clone(), sort.clone(), scan_gen.clone());
        ui.on_rescan_folder(move || {
            let dir = settings.borrow().music_folder.clone();
            match dir {
                Some(dir) => scan_folder(weak.clone(), library.clone(), displayed.clone(), query.clone(), sort.clone(), scan_gen.clone(), dir, true),
                None => {
                    if let Some(ui) = weak.upgrade() { ui.set_status(i18n::tr("Choose a music folder first").into()); }
                }
            }
        });
    }

    let weak = ui_weak.clone();
    ui.on_choose_folder(move || {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
        settings.borrow_mut().music_folder = Some(dir.clone());
        if let Some(ui) = weak.upgrade() { ui.set_music_folder(dir.display().to_string().into()); }
        save_settings(&weak, &settings);
        scan_folder(weak.clone(), library.clone(), displayed.clone(), query.clone(), sort.clone(), scan_gen.clone(), dir, true);
    });

    // App lock: the lock screen shows at start when a code is set, and again when the window is minimized.
    let lock = Rc::new(LockCtl {
        store: RefCell::new(match applock::load() {
            Ok(store) => store,
            Err(e) => {
                ui.set_status(i18n::tr_fmt("Could not read the lock settings: {}", &[&e]).into());
                LockStore::default()
            }
        }),
        pending: RefCell::new(None),
        new_code: RefCell::new(None),
        tick: slint::Timer::default(),
        watch: slint::Timer::default(),
        was_min: Cell::new(false),
    });
    wire_app_lock(&ui, &lock);
    sync_lock_flags(&ui, &lock.store.borrow());
    update_stems_file_info(&ui);
    show_stem_state(&ui);
    if lock.store.borrow().is_enabled() {
        ui.set_locked(true);
        update_lock_wait(&lock, &ui_weak, &ui);
        start_lock_watch(&lock, &ui_weak);
    }

    // Spectrum rays: live (a timer, see `start_rays_timer`) or, in low-power mode, one still picture and no timer.
    apply_low_power_visuals(&ui_weak, &rays_timer, &player, low_power_start);

    ui.run()
}
