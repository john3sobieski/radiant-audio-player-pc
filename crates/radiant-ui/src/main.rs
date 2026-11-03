// No console window in release builds on Windows.
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

//! Radiant UI binary.
//! DONE: window, folder picker, background scan, track list (titles only).
//! TODO: remember the chosen folder (settings file), rescan on start, player controls,
//!       everything in PROGRESS.md. Keep this file thin: logic belongs in the other crates.

slint::include_modules!();

use slint::{ModelRc, SharedString, VecModel};

fn main() -> Result<(), slint::PlatformError> {
    let ui = MainWindow::new()?;

    let weak = ui.as_weak();
    ui.on_choose_folder(move || {
        let Some(dir) = rfd::FileDialog::new().pick_folder() else { return };
        if let Some(ui) = weak.upgrade() {
            ui.set_status("Scanning...".into());
        }
        let weak = weak.clone();
        // Scan off the UI thread, then hand the result back to the event loop.
        std::thread::spawn(move || {
            let tracks = radiant_core::library::scan(&dir);
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
