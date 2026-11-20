// No console window on Windows (release builds).
#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

//! Radiant setup. One exe, three roles:
//! - with a payload glued to its end (`pack`): the installer;
//! - copied into the app folder as `uninstall.exe` (without payload): the uninstaller, which relaunches itself
//!   from %TEMP% (`--run-uninstall <folder>`) so that it can delete its own folder;
//! - plain build output without payload: shows an error page.
//! See PROGRESS.md, stage 14.

slint::include_modules!();

mod install;
mod payload;
mod uninstall;

use std::cell::RefCell;
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use payload::Payload;
use radiant_core::settings;
use radiant_stems::MODEL;
use slint::ComponentHandle;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let Ok(exe) = std::env::current_exe() else {
        return show_fatal("The path of this program could not be found.");
    };
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();

    // The copy of the uninstaller that runs from %TEMP%.
    if args.first().map_or(false, |a| a == "--run-uninstall") {
        let Some(dir) = args.get(1).map(PathBuf::from) else {
            return show_fatal("The folder to remove was not given.");
        };
        run_uninstaller(dir);
        uninstall::schedule_self_delete();
        return;
    }

    // `uninstall.exe` in the app folder: hand over to the temp copy.
    let stem = exe.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if stem == "uninstall" {
        let Some(dir) = exe.parent().map(|p| p.to_path_buf()) else {
            return show_fatal("The program folder could not be found.");
        };
        if let Err(e) = uninstall::relaunch_from_temp(&exe, &dir) {
            show_fatal(&format!("The uninstaller could not be started: {e}"));
        }
        return;
    }

    match Payload::open(&exe) {
        Ok(Some(payload)) => run_installer(exe, payload),
        Ok(None) => show_fatal("This file has no program inside. Use Radiant-Setup.exe from the build."),
        Err(e) => show_fatal(&format!("The installer file could not be read: {e}")),
    }
}

/// A window with only an error message.
fn show_fatal(message: &str) {
    let Ok(ui) = SetupWindow::new() else { return };
    ui.set_version(VERSION.into());
    ui.set_page(9);
    ui.set_error(message.into());
    let weak = ui.as_weak();
    ui.on_finish(move || {
        if let Some(ui) = weak.upgrade() {
            let _ = ui.hide();
        }
    });
    let _ = ui.run();
}

/// Sets text, bar and the "model is downloading" flag from a worker thread.
fn push_progress(weak: &slint::Weak<SetupWindow>, text: String, value: f32, downloading: bool) {
    let weak = weak.clone();
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(ui) = weak.upgrade() {
            ui.set_status(text.into());
            ui.set_progress(value);
            ui.set_downloading(downloading);
        }
    });
}

fn notes_text(notes: &[String]) -> String {
    notes.join("\n\n")
}

// ---------------------------------------------------------------------------------------------------------------
// Installer
// ---------------------------------------------------------------------------------------------------------------

fn run_installer(exe: PathBuf, payload: Payload) {
    for need in install::REQUIRED {
        if !payload.entries.iter().any(|e| e.name == need) {
            return show_fatal(&format!("The installer is incomplete: {need} is missing inside it."));
        }
    }
    let Ok(ui) = SetupWindow::new() else { return };
    ui.set_version(VERSION.into());
    ui.set_page(0);
    ui.set_space_needed(format!("{} MB", install::mb(payload.raw_total())).into());
    ui.set_model_size(format!("{} MB", install::mb(MODEL.bytes.unwrap_or(MODEL.approx_bytes))).into());
    ui.set_model_available(MODEL.is_pinned());

    // Defaults. A new setup over an old install starts from what is already there.
    let dir = install::previous_install_dir().unwrap_or_else(install::default_install_dir);
    ui.set_install_dir(dir.display().to_string().into());
    let existing = settings::load().unwrap_or_default();
    let music = existing.music_folder.clone().or_else(install::default_music_dir);
    ui.set_music_dir(music.map(|m| m.display().to_string()).unwrap_or_default().into());
    ui.set_language_index(install::index_from_language(&existing.language));

    {
        let weak = ui.as_weak();
        ui.on_browse_install(move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut dialog = rfd::FileDialog::new();
            let current = PathBuf::from(ui.get_install_dir().to_string());
            if let Some(parent) = current.ancestors().find(|p| p.is_dir()) {
                dialog = dialog.set_directory(parent);
            }
            let Some(picked) = dialog.pick_folder() else { return };
            ui.set_install_dir(install::folder_for_pick(picked).display().to_string().into());
            ui.set_error("".into());
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_browse_music(move || {
            let Some(ui) = weak.upgrade() else { return };
            let mut dialog = rfd::FileDialog::new();
            let current = PathBuf::from(ui.get_music_dir().to_string());
            if let Some(parent) = current.ancestors().find(|p| p.is_dir()) {
                dialog = dialog.set_directory(parent);
            }
            let Some(picked) = dialog.pick_folder() else { return };
            ui.set_music_dir(picked.display().to_string().into());
            ui.set_error("".into());
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_go_back(move || {
            let Some(ui) = weak.upgrade() else { return };
            ui.set_error("".into());
            let page = ui.get_page();
            if page >= 1 && page <= 3 {
                ui.set_page(page - 1);
            }
        });
    }

    // Where the program went (for "Launch Radiant" on the last page).
    let installed_to: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));
    let cancel = Arc::new(AtomicBool::new(false));
    drop(payload);

    {
        let weak = ui.as_weak();
        let installed_to = installed_to.clone();
        let cancel = cancel.clone();
        ui.on_go_next(move || {
            let Some(ui) = weak.upgrade() else { return };
            match ui.get_page() {
                0 => match install::validate_dir(&ui.get_install_dir()) {
                    Ok(dir) => {
                        ui.set_install_dir(dir.display().to_string().into());
                        ui.set_error("".into());
                        ui.set_page(1);
                    }
                    Err(e) => ui.set_error(e.into()),
                },
                1 => match install::validate_music(&ui.get_music_dir()) {
                    Ok(_) => {
                        ui.set_error("".into());
                        ui.set_page(2);
                    }
                    Err(e) => ui.set_error(e.into()),
                },
                2 => ui.set_page(3),
                3 => {
                    let Ok(dir) = install::validate_dir(&ui.get_install_dir()) else {
                        ui.set_page(0);
                        return;
                    };
                    let music = install::validate_music(&ui.get_music_dir()).ok().flatten();
                    let choices = install::Choices {
                        dir: dir.clone(),
                        music,
                        language: install::language_from_index(ui.get_language_index()),
                        desktop: ui.get_opt_desktop(),
                        start_menu: ui.get_opt_start(),
                        model: ui.get_opt_model() && MODEL.is_pinned(),
                    };
                    *installed_to.borrow_mut() = Some(dir);
                    cancel.store(false, Ordering::Relaxed);
                    ui.set_error("".into());
                    ui.set_status("Preparing…".into());
                    ui.set_progress(0.0);
                    ui.set_downloading(false);
                    ui.set_page(4);
                    start_install(weak.clone(), exe.clone(), choices, cancel.clone());
                }
                _ => {}
            }
        });
    }
    {
        let cancel = cancel.clone();
        ui.on_skip_model(move || cancel.store(true, Ordering::Relaxed));
    }
    {
        let weak = ui.as_weak();
        let installed_to = installed_to.clone();
        ui.on_finish(move || {
            let Some(ui) = weak.upgrade() else { return };
            if ui.get_page() == 5 && ui.get_opt_launch() {
                if let Some(dir) = installed_to.borrow().as_ref() {
                    let _ = std::process::Command::new(dir.join(install::EXE_NAME)).current_dir(dir).spawn();
                }
            }
            let _ = ui.hide();
        });
    }
    let _ = ui.run();
}

/// The installation runs in a worker thread; the window gets progress through `invoke_from_event_loop`.
fn start_install(weak: slint::Weak<SetupWindow>, exe: PathBuf, choices: install::Choices, cancel: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        // The worker reads the list of packed files itself (only the small header, not the files).
        let payload = match Payload::open(&exe) {
            Ok(Some(p)) => p,
            _ => {
                let weak = weak.clone();
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(ui) = weak.upgrade() {
                        ui.set_error("The installer file could not be read again.".into());
                        ui.set_page(3);
                    }
                });
                return;
            }
        };

        let mut last = Instant::now().checked_sub(Duration::from_secs(1)).unwrap_or_else(Instant::now);
        let mut last_flag = false;
        let progress_weak = weak.clone();
        let mut report = |text: &str, value: f32, downloading: bool| {
            // The download reports very often: the window gets about 12 updates a second.
            let force = value >= 1.0 || downloading != last_flag;
            if !force && last.elapsed() < Duration::from_millis(80) {
                return;
            }
            last = Instant::now();
            last_flag = downloading;
            push_progress(&progress_weak, text.to_string(), value, downloading);
        };
        let result = install::run(&exe, &payload, &choices, &cancel, &mut report);

        let _ = slint::invoke_from_event_loop(move || {
            let Some(ui) = weak.upgrade() else { return };
            match result {
                Ok(notes) => {
                    ui.set_notes(notes_text(&notes).into());
                    ui.set_downloading(false);
                    ui.set_page(5);
                }
                // Back to the last step with the reason: the user can fix it (close Radiant, other folder) and retry.
                Err(message) => {
                    ui.set_downloading(false);
                    ui.set_error(message.into());
                    ui.set_page(3);
                }
            }
        });
    });
}

// ---------------------------------------------------------------------------------------------------------------
// Uninstaller
// ---------------------------------------------------------------------------------------------------------------

fn run_uninstaller(app_dir: PathBuf) {
    let Ok(ui) = SetupWindow::new() else { return };
    ui.set_uninstall_mode(true);
    ui.set_caption("Radiant Uninstall".into());
    ui.set_version(VERSION.into());
    ui.set_page(10);
    ui.set_app_dir(app_dir.display().to_string().into());
    ui.set_opt_remove_data(false);

    let info = match uninstall::data_dir().filter(|d| d.exists()) {
        Some(dir) => format!(
            "Deletes {} ({} MB): settings, playlists, queues, app lock, Dropbox sign-in and the AI model. Your music files stay where they are.",
            dir.display(),
            install::mb(uninstall::dir_size(&dir))
        ),
        None => "There is no settings folder to delete.".to_string(),
    };
    ui.set_data_info(info.into());

    {
        let weak = ui.as_weak();
        let app_dir = app_dir.clone();
        ui.on_start_uninstall(move || {
            let Some(ui) = weak.upgrade() else { return };
            let remove_data = ui.get_opt_remove_data();
            ui.set_error("".into());
            ui.set_status("Removing the program…".into());
            ui.set_progress(0.0);
            ui.set_page(11);
            let weak = weak.clone();
            let app_dir = app_dir.clone();
            std::thread::spawn(move || {
                let progress_weak = weak.clone();
                let mut report = |text: &str, value: f32| push_progress(&progress_weak, text.to_string(), value, false);
                let result = uninstall::run(&app_dir, remove_data, &mut report);
                let _ = slint::invoke_from_event_loop(move || {
                    let Some(ui) = weak.upgrade() else { return };
                    match result {
                        Ok(notes) => {
                            let text = if notes.is_empty() { "Radiant has been removed from this computer.".to_string() } else { notes_text(&notes) };
                            ui.set_notes(text.into());
                            ui.set_page(12);
                        }
                        Err(uninstall::RemoveError::InUse(message)) => {
                            ui.set_error(message.into());
                            ui.set_page(10);
                        }
                    }
                });
            });
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_finish(move || {
            if let Some(ui) = weak.upgrade() {
                let _ = ui.hide();
            }
        });
    }
    let _ = ui.run();
}
