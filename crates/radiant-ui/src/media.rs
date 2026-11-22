//! Stage 13: system media controls (Windows: SMTC; Linux: MPRIS over D-Bus): media keys of the keyboard, the volume flyout, the lock screen and
//! Bluetooth headsets show the playing track and drive Radiant (play / pause / next / previous / seek).
//!
//! Everything lives on the UI thread: `init` is called once after the window exists, `set_track` / `set_state` are
//! called from the engine-status handler (already on the UI thread). Button presses arrive on a Windows thread and
//! are forwarded to the UI thread, where they call the same callbacks as the buttons on the screen (`invoke_*`).
//! Not covered (yet): cover art in the system panel (needs a file or http URL, embedded covers have none).
//! Other systems (macOS): empty stubs, nothing happens.

use crate::MainWindow;
use radiant_audio::PlayState;
use radiant_core::models::Track;
use std::path::Path;

#[cfg(any(windows, target_os = "linux"))]
mod imp {
    use super::*;
    use slint::ComponentHandle;
    use souvlaki::{
        MediaControlEvent, MediaControls, MediaMetadata, MediaPlayback, MediaPosition, PlatformConfig, SeekDirection,
    };
    use std::cell::RefCell;
    use std::time::{Duration, Instant};

    struct Media {
        controls: MediaControls,
        /// What Windows was told last: state, position, when.
        last_state: Option<PlayState>,
        last_pos_ms: u64,
        last_sent: Instant,
    }

    thread_local! {
        static MEDIA: RefCell<Option<Media>> = const { RefCell::new(None) };
    }

    /// Windows: SMTC needs the window handle, which exists only once the window is shown (None = try again later).
    #[cfg(windows)]
    fn platform_config(ui: &MainWindow) -> Option<PlatformConfig<'static>> {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let handle = ui.window().window_handle();
        let hwnd = match handle.window_handle().map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(h)) => h.hwnd.get() as *mut std::ffi::c_void,
            _ => return None,
        };
        Some(PlatformConfig { dbus_name: "radiant", display_name: "Radiant", hwnd: Some(hwnd) })
    }

    /// Linux: MPRIS needs no window; the bus name is org.mpris.MediaPlayer2.<dbus_name> (the app-id, as Flatpak wants).
    #[cfg(target_os = "linux")]
    fn platform_config(_ui: &MainWindow) -> Option<PlatformConfig<'static>> {
        Some(PlatformConfig { dbus_name: "io.github.john3sobieski.Radiant", display_name: "Radiant", hwnd: None })
    }

    /// Creates the controls. Returns false when the platform is not ready yet (the caller may try again).
    fn init(ui: &MainWindow) -> bool {
        let Some(config) = platform_config(ui) else { return false };
        let Ok(mut controls) = MediaControls::new(config) else { return true }; // no SMTC / no D-Bus session: keys only in the window
        let weak = ui.as_weak();
        let attached = controls.attach(move |event| {
            let _ = weak.upgrade_in_event_loop(move |ui| handle_event(&ui, event));
        });
        if attached.is_err() {
            return true;
        }
        MEDIA.with(|m| {
            *m.borrow_mut() = Some(Media { controls, last_state: None, last_pos_ms: 0, last_sent: Instant::now() });
        });
        true
    }

    /// Button of the system panel -> the same action as the button on the screen.
    fn handle_event(ui: &MainWindow, event: MediaControlEvent) {
        let playing = ui.get_is_playing();
        let secs = ui.get_duration_secs();
        let now = ui.get_progress() * secs;
        let seek_to = |target: f32| {
            if secs > 0.0 {
                ui.invoke_seek((target / secs).clamp(0.0, 1.0));
            }
        };
        match event {
            MediaControlEvent::Play => if !playing { ui.invoke_toggle_play() },
            MediaControlEvent::Pause | MediaControlEvent::Stop => if playing { ui.invoke_toggle_play() },
            MediaControlEvent::Toggle => ui.invoke_toggle_play(),
            MediaControlEvent::Next => ui.invoke_next_track(),
            MediaControlEvent::Previous => ui.invoke_previous_track(),
            MediaControlEvent::SetPosition(MediaPosition(d)) => seek_to(d.as_secs_f32()),
            MediaControlEvent::Seek(dir) => seek_to(match dir {
                SeekDirection::Forward => now + 10.0,
                SeekDirection::Backward => now - 10.0,
            }),
            MediaControlEvent::SeekBy(dir, d) => seek_to(match dir {
                SeekDirection::Forward => now + d.as_secs_f32(),
                SeekDirection::Backward => now - d.as_secs_f32(),
            }),
            _ => {}
        }
    }

    pub fn start(ui: &MainWindow) {
        // Windows: the window handle exists only once the window is shown: try on the first loop turns, then give up quietly.
        let weak = ui.as_weak();
        let tries = std::rc::Rc::new(std::cell::Cell::new(0_u32));
        let timer = std::rc::Rc::new(slint::Timer::default());
        let timer2 = timer.clone();
        timer.start(slint::TimerMode::Repeated, Duration::from_millis(250), move || {
            tries.set(tries.get() + 1);
            let done = weak.upgrade().map_or(true, |ui| init(&ui));
            if done || tries.get() >= 20 {
                timer2.stop();
            }
        });
        // Keep the timer alive until it stops itself.
        std::mem::forget(timer);
    }

    pub fn set_track(track: Option<&Track>, path: Option<&Path>) {
        MEDIA.with(|m| {
            let mut m = m.borrow_mut();
            let Some(m) = m.as_mut() else { return };
            let fallback = path.and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned());
            let title = track.map(|t| t.title.clone()).or(fallback);
            let _ = m.controls.set_metadata(MediaMetadata {
                title: title.as_deref(),
                artist: track.and_then(|t| t.artist.as_deref()),
                album: track.and_then(|t| t.album.as_deref()),
                cover_url: None,
                duration: track.and_then(|t| t.duration_ms).map(Duration::from_millis),
            });
            m.last_state = None; // force the next `set_state` through
        });
    }

    pub fn set_state(state: PlayState, position_ms: u64) {
        MEDIA.with(|m| {
            let mut m = m.borrow_mut();
            let Some(m) = m.as_mut() else { return };
            // The status comes many times a second: tell the system on a change, on a jump (seek) and every ~3 s.
            let jumped = position_ms < m.last_pos_ms || position_ms > m.last_pos_ms + 6000;
            if m.last_state == Some(state) && !jumped && m.last_sent.elapsed() < Duration::from_secs(3) {
                return;
            }
            let progress = Some(MediaPosition(Duration::from_millis(position_ms)));
            let playback = match state {
                PlayState::Playing => MediaPlayback::Playing { progress },
                PlayState::Paused => MediaPlayback::Paused { progress },
                PlayState::Stopped => MediaPlayback::Stopped,
            };
            let _ = m.controls.set_playback(playback);
            m.last_state = Some(state);
            m.last_pos_ms = position_ms;
            m.last_sent = Instant::now();
        });
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod imp {
    use super::*;
    pub fn start(_ui: &MainWindow) {}
    pub fn set_track(_track: Option<&Track>, _path: Option<&Path>) {}
    pub fn set_state(_state: PlayState, _position_ms: u64) {}
}

/// Call once at start (after the window is built); the controls appear as soon as the window handle exists.
pub fn start(ui: &MainWindow) {
    imp::start(ui);
}

/// A new track started (`track` is None when it is not in the library: the file name is used).
pub fn set_track(track: Option<&Track>, path: Option<&Path>) {
    imp::set_track(track, path);
}

/// Play state and position, from every engine status.
pub fn set_state(state: PlayState, position_ms: u64) {
    imp::set_state(state, position_ms);
}
