// No console window in release builds on Windows.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

//! Radiant UI binary.
//! DONE: window, folder picker, background scan, track list, playback buttons wired to the engine.
//! TODO: remember the chosen folder (settings file), rescan on start, seek bar, shuffle/repeat
//!       buttons, everything in PROGRESS.md. Keep this file thin: logic belongs in the other crates.

slint::include_modules!();

use radiant_audio::{Command, PlayState, Player};
use slint::{ModelRc, SharedString, VecModel};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;

    // Paths of the tracks shown in the list, in the same order (shared with the scan thread).
    let library: Arc<Mutex<Vec<PathBuf>>> = Arc::default();

    // The engine reports changes from its own thread; forward them to the UI thread.
    let weak = ui.as_weak();
    let player = Rc::new(Player::spawn(move |status| {
        let _ = weak.upgrade_in_event_loop(move |ui| {
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
