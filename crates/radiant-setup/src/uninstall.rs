//! Removing Radiant: the files the setup put into the app folder, the shortcuts, the "Apps & features" entry and,
//! if the user ticks it, the data folder (%APPDATA%\Radiant). The music folder is never touched.
//!
//! A running exe cannot delete itself or its folder, so `uninstall.exe` first copies itself to %TEMP% and starts
//! that copy (`--run-uninstall <folder>`); the copy does the work and asks `cmd` to delete it after it exits.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use radiant_core::settings;
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use crate::install::{APP_FILES, REG_KEY, UNINSTALLER_NAME};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub enum RemoveError {
    /// Radiant (or a file of it) is in use: the user can close it and try again, nothing else was removed yet.
    InUse(String),
    Other(String),
}

/// `%APPDATA%\Radiant`: settings, playlists, queues, app lock, Dropbox sign-in, the AI model.
pub fn data_dir() -> Option<PathBuf> {
    settings::settings_path().parent().map(Path::to_path_buf)
}

pub fn dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    total += dir_size(&entry.path());
                } else {
                    total += meta.len();
                }
            }
        }
    }
    total
}

/// Starts a copy of this exe from %TEMP% that does the uninstall. The caller exits right after.
pub fn relaunch_from_temp(exe: &Path, app_dir: &Path) -> Result<(), String> {
    let temp = std::env::temp_dir().join(format!("radiant-uninstall-{}.exe", std::process::id()));
    fs::copy(exe, &temp).map_err(|e| format!("{}: {e}", temp.display()))?;
    Command::new(&temp).arg("--run-uninstall").arg(app_dir).spawn().map_err(|e| format!("{}: {e}", temp.display()))?;
    Ok(())
}

/// Deletes the running exe a moment after the process has ended (called from the temp copy as the last thing).
pub fn schedule_self_delete() {
    let Ok(exe) = std::env::current_exe() else { return };
    let line = format!("/C ping -n 4 127.0.0.1 >nul & del /f /q \"{}\"", exe.display());
    let _ = Command::new("cmd")
        .raw_arg(line)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

fn is_in_use(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5) | Some(32)) || e.kind() == std::io::ErrorKind::PermissionDenied
}

/// String values `<prefix>0`, `<prefix>1`, ... of the setup's registry key.
fn recorded(prefix: &str) -> Vec<String> {
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(REG_KEY) else { return Vec::new() };
    let mut out = Vec::new();
    for value in key.enum_values() {
        let Ok((name, _)) = value else { continue };
        if !name.starts_with(prefix) {
            continue;
        }
        if let Ok(text) = key.get_value::<String, _>(&name) {
            out.push(text);
        }
    }
    out
}

/// The shortcuts the setup recorded (only `.lnk` files are ever deleted).
fn recorded_shortcuts() -> Vec<PathBuf> {
    recorded("Shortcut")
        .into_iter()
        .map(PathBuf::from)
        .filter(|p| p.extension().map_or(false, |e| e.eq_ignore_ascii_case("lnk")))
        .collect()
}

/// The files to remove from the app folder, `radiant.exe` first (if it is running, nothing else is touched).
/// The fallback list plus every plain file name the setup recorded; the uninstaller itself is removed last, separately.
fn program_files() -> Vec<String> {
    let mut files: Vec<String> = APP_FILES.iter().filter(|n| **n != UNINSTALLER_NAME).map(|n| n.to_string()).collect();
    for name in recorded("File") {
        let plain = !name.is_empty() && Path::new(&name).file_name().map_or(false, |n| n == std::ffi::OsStr::new(&name));
        if plain && !name.eq_ignore_ascii_case(UNINSTALLER_NAME) && !files.iter().any(|f| f.eq_ignore_ascii_case(&name)) {
            files.push(name);
        }
    }
    files
}

/// Removes everything. `report(text, bar 0..1)`. `Ok(notes)` = removed; the notes are leftovers worth telling.
pub fn run(app_dir: &Path, remove_data: bool, report: &mut dyn FnMut(&str, f32)) -> Result<Vec<String>, RemoveError> {
    let mut notes: Vec<String> = Vec::new();
    let links = recorded_shortcuts();
    let files = program_files();

    // The program first: if it is running, this fails before anything else is touched and the user can retry.
    report("Removing the program…", 0.1);
    for name in &files {
        let path = app_dir.join(name);
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) if is_in_use(&e) => {
                return Err(RemoveError::InUse(format!("{name} is in use. Close Radiant and press Remove again.")))
            }
            Err(e) => notes.push(format!("{} could not be removed ({e}).", path.display())),
        }
    }

    report("Removing the shortcuts…", 0.4);
    for link in &links {
        if let Err(e) = fs::remove_file(link) {
            if e.kind() != std::io::ErrorKind::NotFound {
                notes.push(format!("{} could not be removed ({e}).", link.display()));
            }
        }
    }

    report("Removing the registry entry…", 0.55);
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(REG_KEY);

    if remove_data {
        report("Deleting the settings and data…", 0.7);
        if let Some(dir) = data_dir() {
            if dir.exists() {
                if let Err(e) = fs::remove_dir_all(&dir) {
                    notes.push(format!("{} could not be deleted completely ({e}).", dir.display()));
                }
            }
        }
    } else if let Some(dir) = data_dir().filter(|d| d.exists()) {
        notes.push(format!("Your settings, playlists and the AI model were kept in {}. Delete that folder to remove them too.", dir.display()));
    }

    // The original uninstall.exe may still be closing: try for a few seconds.
    report("Removing the uninstaller…", 0.85);
    let uninstaller = app_dir.join(UNINSTALLER_NAME);
    let mut removed = false;
    for _ in 0..25 {
        match fs::remove_file(&uninstaller) {
            Ok(()) => {
                removed = true;
                break;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                removed = true;
                break;
            }
            Err(_) => std::thread::sleep(Duration::from_millis(200)),
        }
    }
    if !removed {
        notes.push(format!("{} could not be removed. Delete it by hand.", uninstaller.display()));
    }

    // Only an empty folder is removed: if the user installed into a folder with other files, they stay.
    if fs::remove_dir(app_dir).is_err() && app_dir.exists() {
        notes.push(format!("The folder {} was left because it still contains other files.", app_dir.display()));
    }

    report("Done", 1.0);
    Ok(notes)
}
