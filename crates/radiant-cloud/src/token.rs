//! The refresh token lives in `dropbox.json` next to `settings.json` (`%APPDATA%/Radiant`). It is stored as plain
//! text, like the other settings: it can only read the app folder of this one Dropbox app, and "Disconnect" revokes it.

use std::fs;
use std::path::{Path, PathBuf};

use crate::CloudError;

const FILE_NAME: &str = "dropbox.json";

fn file(dir: &Path) -> PathBuf {
    dir.join(FILE_NAME)
}

/// The stored refresh token, if there is one.
pub fn load_token(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(file(dir)).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    v["refresh_token"].as_str().filter(|t| !t.is_empty()).map(str::to_string)
}

/// Saves the token (written to a temporary file first, so a crash cannot leave half a file).
pub fn save_token(dir: &Path, token: &str) -> Result<(), CloudError> {
    fs::create_dir_all(dir)?;
    let text = serde_json::json!({ "refresh_token": token }).to_string();
    let target = file(dir);
    let tmp = dir.join(format!("{FILE_NAME}.tmp"));
    fs::write(&tmp, text)?;
    fs::rename(&tmp, &target)?;
    Ok(())
}

/// Removes the stored token (no error when there is none).
pub fn clear_token(dir: &Path) -> Result<(), CloudError> {
    match fs::remove_file(file(dir)) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_round_trip() {
        let dir = std::env::temp_dir().join(format!("radiant-cloud-test-{}", std::process::id()));
        assert_eq!(load_token(&dir), None);
        save_token(&dir, "abc-123").unwrap();
        assert_eq!(load_token(&dir).as_deref(), Some("abc-123"));
        save_token(&dir, "def").unwrap();
        assert_eq!(load_token(&dir).as_deref(), Some("def"));
        clear_token(&dir).unwrap();
        clear_token(&dir).unwrap();
        assert_eq!(load_token(&dir), None);
        let _ = fs::remove_dir_all(&dir);
    }
}
