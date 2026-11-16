//! The installation itself: unpack the files, remember the language and music folder in the app's settings, make
//! the shortcuts, add the "Apps & features" entry, optionally download the AI model.
//! Everything is per user (HKCU, %LOCALAPPDATA%): no administrator rights.

use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;

use radiant_core::settings::{self, LANGUAGE_CODES, LANGUAGE_SYSTEM};
use radiant_stems::{ModelState, ModelStore, Progress, StemError, MODEL};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use crate::payload::{self, Payload};

pub const EXE_NAME: &str = "radiant.exe";
pub const UNINSTALLER_NAME: &str = "uninstall.exe";
/// What the program needs next to its exe (the setup refuses to run without them).
pub const REQUIRED: [&str; 3] = ["radiant.exe", "onnxruntime.dll", "DirectML.dll"];
/// Fallback list for the uninstaller. The exact list of installed files is recorded in the registry (`File0..`),
/// so a file added to the payload needs no change here.
pub const APP_FILES: [&str; 5] = ["radiant.exe", "onnxruntime.dll", "DirectML.dll", "LICENSE.md", "uninstall.exe"];
pub const REG_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Radiant";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// What the user chose in the wizard.
pub struct Choices {
    pub dir: PathBuf,
    pub music: Option<PathBuf>,
    /// `"system"` or one of the language codes of the app.
    pub language: String,
    pub desktop: bool,
    pub start_menu: bool,
    pub model: bool,
}

/// Bytes -> whole megabytes (decimal, as the app shows them).
pub fn mb(bytes: u64) -> u64 {
    (bytes + 500_000) / 1_000_000
}

pub fn language_from_index(index: i32) -> String {
    if index <= 0 {
        return LANGUAGE_SYSTEM.to_string();
    }
    LANGUAGE_CODES.get(index as usize - 1).map(|c| c.to_string()).unwrap_or_else(|| LANGUAGE_SYSTEM.to_string())
}

pub fn index_from_language(language: &str) -> i32 {
    LANGUAGE_CODES.iter().position(|c| *c == language).map(|i| i as i32 + 1).unwrap_or(0)
}

pub fn default_install_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Users\Public"));
    base.join("Programs").join("Radiant")
}

/// Where Radiant was installed before (so a new setup updates it in place).
pub fn previous_install_dir() -> Option<PathBuf> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(REG_KEY).ok()?;
    let value: String = key.get_value("InstallLocation").ok()?;
    if value.is_empty() {
        None
    } else {
        Some(PathBuf::from(value))
    }
}

pub fn default_music_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("USERPROFILE")?).join("Music");
    dir.is_dir().then_some(dir)
}

/// The folder picked in the dialog. A non-empty folder that is not an old Radiant install gets a `Radiant`
/// subfolder, so the program does not land among someone's other files.
pub fn folder_for_pick(picked: PathBuf) -> PathBuf {
    let non_empty = fs::read_dir(&picked).map(|mut it| it.next().is_some()).unwrap_or(false);
    if non_empty && !picked.join(EXE_NAME).exists() {
        picked.join("Radiant")
    } else {
        picked
    }
}

pub fn validate_dir(text: &str) -> Result<PathBuf, String> {
    let text = text.trim().trim_matches('"');
    if text.is_empty() {
        return Err("Choose the install folder.".into());
    }
    let path = PathBuf::from(text);
    if !path.is_absolute() {
        return Err(r"Type the full path, for example C:\Programs\Radiant.".into());
    }
    if path.parent().is_none() {
        return Err("Choose a folder, not a whole drive.".into());
    }
    if path.exists() && !path.is_dir() {
        return Err("This path is a file, not a folder.".into());
    }
    Ok(path)
}

/// The music folder is optional: empty = choose it later in the app.
pub fn validate_music(text: &str) -> Result<Option<PathBuf>, String> {
    let text = text.trim().trim_matches('"');
    if text.is_empty() {
        return Ok(None);
    }
    let path = PathBuf::from(text);
    if !path.is_dir() {
        return Err("This music folder does not exist. Pick an existing folder or leave the field empty.".into());
    }
    Ok(Some(path))
}

fn write_error(path: &Path, e: &std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        format!(
            "Windows does not allow writing to {}. Close Radiant if it is running, or choose another folder (for example one inside your user folder).",
            path.display()
        )
    } else {
        format!("{}: {e}", path.display())
    }
}

/// A payload name must be a plain file name: nothing can be unpacked outside the app folder.
fn plain_name(name: &str) -> bool {
    !name.is_empty() && Path::new(name).file_name().map_or(false, |n| n == std::ffi::OsStr::new(name))
}

fn models_dir() -> PathBuf {
    settings::settings_path().parent().map(|p| p.join("models")).unwrap_or_else(|| PathBuf::from("models"))
}

/// Language and music folder go into the app's own `settings.json`; everything else in it stays as it is.
fn save_settings(c: &Choices) -> Result<(), String> {
    let mut s = settings::load()?;
    s.language = c.language.clone();
    if let Some(music) = &c.music {
        s.music_folder = Some(music.clone());
    }
    settings::save(&s)
}

const LNK_SCRIPT: &str = "$ErrorActionPreference = 'Stop'; \
$dir = [Environment]::GetFolderPath($env:RADIANT_LNK_FOLDER); \
$lnk = Join-Path $dir 'Radiant.lnk'; \
$s = (New-Object -ComObject WScript.Shell).CreateShortcut($lnk); \
$s.TargetPath = $env:RADIANT_LNK_TARGET; \
$s.WorkingDirectory = $env:RADIANT_LNK_WORKDIR; \
$s.IconLocation = $env:RADIANT_LNK_TARGET + ',0'; \
$s.Description = 'Radiant music player'; \
$s.Save(); \
[System.IO.File]::WriteAllText($env:RADIANT_LNK_OUT, $lnk)";

/// Creates `Radiant.lnk` in a Windows special folder (`Desktop` or `Programs` = the user's Start menu) and returns
/// its path. The special folder is asked from Windows (the desktop is often moved to OneDrive). All values go
/// through environment variables, so non-Latin paths are not damaged by command-line encoding.
fn make_shortcut(folder: &str, target: &Path, workdir: &Path) -> Result<PathBuf, String> {
    let powershell = std::env::var_os("SystemRoot")
        .map(|r| PathBuf::from(r).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("powershell"));
    let out = std::env::temp_dir().join(format!("radiant-lnk-{}-{}.txt", std::process::id(), folder));
    let status = Command::new(powershell)
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", LNK_SCRIPT])
        .env("RADIANT_LNK_FOLDER", folder)
        .env("RADIANT_LNK_TARGET", target)
        .env("RADIANT_LNK_WORKDIR", workdir)
        .env("RADIANT_LNK_OUT", &out)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("PowerShell could not be started ({e})"))?;
    let path = fs::read_to_string(&out).ok().map(|t| PathBuf::from(t.trim()));
    let _ = fs::remove_file(&out);
    match (status.success(), path) {
        (true, Some(p)) if !p.as_os_str().is_empty() => Ok(p),
        _ => Err("PowerShell reported an error".into()),
    }
}

fn set_str(key: &RegKey, name: &str, value: &str) -> Result<(), String> {
    key.set_value(name, &value.to_string()).map_err(|e| e.to_string())
}

/// The "Apps & features" entry. The shortcut paths and the names of all installed files are kept here too, so the
/// uninstaller deletes exactly them.
fn register(c: &Choices, version: &str, links: &[PathBuf], files: &[String], size_kb: u32) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(REG_KEY);
    let (key, _) = hkcu.create_subkey(REG_KEY).map_err(|e| e.to_string())?;
    let exe = c.dir.join(EXE_NAME);
    let uninstaller = c.dir.join(UNINSTALLER_NAME);
    set_str(&key, "DisplayName", "Radiant")?;
    set_str(&key, "DisplayVersion", version)?;
    set_str(&key, "Publisher", "john3sobieski")?;
    set_str(&key, "InstallLocation", &c.dir.display().to_string())?;
    set_str(&key, "DisplayIcon", &exe.display().to_string())?;
    set_str(&key, "UninstallString", &format!("\"{}\"", uninstaller.display()))?;
    key.set_value("NoModify", &1u32).map_err(|e| e.to_string())?;
    key.set_value("NoRepair", &1u32).map_err(|e| e.to_string())?;
    key.set_value("EstimatedSize", &size_kb).map_err(|e| e.to_string())?;
    for (i, link) in links.iter().enumerate() {
        set_str(&key, &format!("Shortcut{i}"), &link.display().to_string())?;
    }
    for (i, file) in files.iter().enumerate() {
        set_str(&key, &format!("File{i}"), file)?;
    }
    Ok(())
}

/// Downloads the model with the same code the app uses (resumable, SHA-256 checked, lands in the same folder).
fn download_model(cancel: &AtomicBool, report: &mut dyn FnMut(&str, f32, bool), start: f32) -> Result<(), StemError> {
    let store = ModelStore::new(models_dir());
    if store.state() == ModelState::Ready {
        return Ok(());
    }
    let span = 1.0 - start;
    let mut on_progress = |p: Progress| match p {
        Progress::Download { done, total } => {
            let f = if total == 0 { 0.0 } else { done as f32 / total as f32 };
            let text = format!("Downloading the AI model… {} of {} MB ({}%)", mb(done), mb(total), (f * 100.0) as u32);
            report(&text, start + span * 0.97 * f, true);
        }
        Progress::Verify { done, total } => {
            let f = if total == 0 { 0.0 } else { done as f32 / total as f32 };
            let text = format!("Checking the downloaded file… {}%", (f * 100.0) as u32);
            report(&text, start + span * (0.97 + 0.03 * f), true);
        }
    };
    store.download(cancel, &mut on_progress)
}

/// Runs the whole installation. `report(text, bar 0..1, model_is_downloading)` is called as it goes.
/// `Err` = the program could not be installed. `Ok(notes)` = installed; the notes are things that did not work out
/// (a shortcut, the model) and are shown on the last page.
pub fn run(
    exe: &Path,
    payload: &Payload,
    c: &Choices,
    cancel: &AtomicBool,
    report: &mut dyn FnMut(&str, f32, bool),
) -> Result<Vec<String>, String> {
    let mut notes: Vec<String> = Vec::new();
    // Share of the bar for the file work; with the model the rest of the bar is the download.
    let files_end: f32 = if c.model { 0.08 } else { 0.9 };

    report("Preparing…", 0.0, false);
    fs::create_dir_all(&c.dir).map_err(|e| write_error(&c.dir, &e))?;

    let total = payload.raw_total().max(1) as f32;
    let mut done = 0f32;
    for entry in &payload.entries {
        if !plain_name(&entry.name) {
            return Err("The installer file is damaged (bad file name inside).".into());
        }
        report(&format!("Copying {}…", entry.name), files_end * 0.9 * done / total, false);
        let dest = c.dir.join(&entry.name);
        payload.extract(exe, entry, &dest).map_err(|e| write_error(&dest, &e))?;
        done += entry.raw_len as f32;
    }

    report("Writing the uninstaller…", files_end * 0.92, false);
    let uninstaller = c.dir.join(UNINSTALLER_NAME);
    payload::write_stub(exe, payload.stub_len, &uninstaller).map_err(|e| write_error(&uninstaller, &e))?;

    report("Saving the language and the music folder…", files_end * 0.94, false);
    if let Err(e) = save_settings(c) {
        notes.push(format!("The language and the music folder could not be saved ({e}). Choose them in the app."));
    }

    let target = c.dir.join(EXE_NAME);
    let mut links: Vec<PathBuf> = Vec::new();
    for (wanted, folder, label) in [(c.desktop, "Desktop", "desktop"), (c.start_menu, "Programs", "Start menu")] {
        if !wanted {
            continue;
        }
        report(&format!("Creating the {label} shortcut…"), files_end * 0.96, false);
        match make_shortcut(folder, &target, &c.dir) {
            Ok(path) => links.push(path),
            Err(e) => notes.push(format!("The {label} shortcut could not be created ({e}).")),
        }
    }

    report("Registering the program…", files_end * 0.98, false);
    let size_kb = ((payload.raw_total() + payload.stub_len) / 1024).min(u32::MAX as u64) as u32;
    let mut installed: Vec<String> = payload.entries.iter().map(|e| e.name.clone()).collect();
    installed.push(UNINSTALLER_NAME.to_string());
    if let Err(e) = register(c, env!("CARGO_PKG_VERSION"), &links, &installed, size_kb) {
        notes.push(format!("Radiant could not be added to the list of installed programs ({e}). Remove it by deleting its folder."));
    }

    if c.model {
        match download_model(cancel, report, files_end) {
            Ok(()) => {}
            Err(StemError::Cancelled) => {
                notes.push("The AI model download was skipped. You can download it later in Settings → AI & stems.".into())
            }
            Err(e) => notes.push(format!("The AI model could not be downloaded ({e}). You can try again in Settings → AI & stems.")),
        }
    }

    report("Done", 1.0, false);
    Ok(notes)
}
