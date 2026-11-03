// No console window in release builds on Windows.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

//! Radiant UI binary.
//! DONE: window, folder picker, background scan, track list, playback buttons wired to the engine.
//! TODO: remember the chosen folder (settings file), rescan on start, shuffle/repeat
//!       buttons, everything in PROGRESS.md. Keep this file thin: logic belongs in the other crates.

slint::include_modules!();

use radiant_audio::{Command, PlayState, Player};
use slint::{ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// 83000 ms -> "1:23"
fn fmt_time(ms: u64) -> String {
    let s = ms / 1000;
    format!("{}:{:02}", s / 60, s % 60)
}

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;

    // Paths of the tracks shown in the list, in the same order (shared with the scan thread).
    let library: Arc<Mutex<Vec<PathBuf>>> = Arc::default();

    // The engine reports changes from its own thread; forward them to the UI thread.
    let weak = ui.as_weak();
    let status_library = library.clone();
    let player = Rc::new(Player::spawn(move |status| {
        // Position of the playing track in the list (-1 = none), for the row highlight.
        let index = status
            .current
            .as_ref()
            .and_then(|c| status_library.lock().ok()?.iter().position(|p| p == c))
            .map_or(-1, |i| i as i32);
        let _ = weak.upgrade_in_event_loop(move |ui| {
            ui.set_current_index(index);
            ui.set_position_text(fmt_time(status.position_ms).into());
            match status.duration_ms {
                Some(d) if d > 0 => {
                    ui.set_duration_text(fmt_time(d).into());
                    ui.set_progress((status.position_ms as f32 / d as f32).clamp(0.0, 1.0));
                }
                _ => {
                    ui.set_duration_text("".into());
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

    {
        let player = player.clone();
        let library = library.clone();
        ui.on_play_track(move |i| {
            if i < 0 {
                return;
            }
            let queue = library.lock().map(|l| l.clone()).unwrap_or_default();
            player.send(Command::SetQueue { queue, start: i as usize });
        });
    }
    {
        let player = player.clone();
        ui.on_toggle_play(move || player.send(Command::Toggle));
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

    let weak = ui.as_weak();
    ui.on_choose_folder(move || {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
        if let Some(ui) = weak.upgrade() {
            ui.set_status("Scanning...".into());
        }
        let weak = weak.clone();
        let library = library.clone();
        // Scan off the UI thread, then hand the result back to the event loop.
        std::thread::spawn(move || {
            let tracks = radiant_core::library::scan(&dir);
            if let Ok(mut l) = library.lock() {
                *l = tracks.iter().map(|t| t.path.clone()).collect();
            }
            let titles: Vec<String> = tracks.into_iter().map(|t| t.title).collect();
            let count = titles.len();
            let _ = weak.upgrade_in_event_loop(move |ui| {
                let model: Vec<SharedString> = titles.into_iter().map(Into::into).collect();
                ui.set_tracks(ModelRc::new(VecModel::from(model)));
                ui.set_status(format!("{count} tracks").into());
            });
        });
    });

    ui.run()
}
