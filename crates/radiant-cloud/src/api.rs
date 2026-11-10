//! The Dropbox HTTP calls: tokens, the file list and the download. Read-only: there is no upload, move or delete
//! call in this crate.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::{redirect_uri, CloudError, APP_KEY};

const TOKEN_URL: &str = "https://api.dropboxapi.com/oauth2/token";
const LIST_URL: &str = "https://api.dropboxapi.com/2/files/list_folder";
const LIST_CONTINUE_URL: &str = "https://api.dropboxapi.com/2/files/list_folder/continue";
const REVOKE_URL: &str = "https://api.dropboxapi.com/2/auth/token/revoke";
const DOWNLOAD_URL: &str = "https://content.dropboxapi.com/2/files/download";

/// One file of the cloud folder.
#[derive(Debug, Clone)]
pub struct RemoteFile {
    /// `id:...` (ASCII, safe for the `Dropbox-API-Arg` header whatever the file name is).
    pub id: String,
    /// Path inside the app folder, without the leading slash, `/` as the separator.
    pub path: String,
    pub size: u64,
}

pub fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout_read(Duration::from_secs(60))
        .timeout_write(Duration::from_secs(30))
        .user_agent("Radiant-for-PC")
        .build()
}

/// A signed-in session: keeps the short-lived access token fresh from the refresh token.
pub struct Session {
    agent: ureq::Agent,
    refresh: String,
    access: String,
    expires: Instant,
}

/// Exchanges the code of the browser sign-in for tokens; returns (refresh token, access token, seconds valid).
pub fn exchange_code(code: &str, verifier: &str) -> Result<(String, String, u64), CloudError> {
    let uri = redirect_uri();
    let result = agent().post(TOKEN_URL).send_form(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", APP_KEY),
        ("redirect_uri", uri.as_str()),
        ("code_verifier", verifier),
    ]);
    let v = read_json(result)?;
    let refresh = v["refresh_token"].as_str().ok_or_else(|| CloudError::Api("no refresh token in the answer".into()))?;
    let access = v["access_token"].as_str().ok_or_else(|| CloudError::Api("no access token in the answer".into()))?;
    let secs = v["expires_in"].as_u64().unwrap_or(3600);
    Ok((refresh.to_string(), access.to_string(), secs))
}

impl Session {
    pub fn start(refresh_token: &str) -> Result<Self, CloudError> {
        let mut session = Session {
            agent: agent(),
            refresh: refresh_token.to_string(),
            access: String::new(),
            expires: Instant::now(),
        };
        session.refresh_access()?;
        Ok(session)
    }

    fn refresh_access(&mut self) -> Result<(), CloudError> {
        let result = self.agent.post(TOKEN_URL).send_form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", self.refresh.as_str()),
            ("client_id", APP_KEY),
        ]);
        let v = read_json(result)?;
        self.access = v["access_token"]
            .as_str()
            .ok_or_else(|| CloudError::Api("no access token in the answer".into()))?
            .to_string();
        self.expires = Instant::now() + Duration::from_secs(v["expires_in"].as_u64().unwrap_or(3600));
        Ok(())
    }

    /// `Bearer ...`, refreshed when it is about to expire.
    fn bearer(&mut self) -> Result<String, CloudError> {
        if Instant::now() + Duration::from_secs(60) >= self.expires {
            self.refresh_access()?;
        }
        Ok(format!("Bearer {}", self.access))
    }

    /// Sends a request; waits and repeats on "too many requests", server errors and dropped connections.
    fn send<F>(&mut self, cancel: &AtomicBool, make: F) -> Result<ureq::Response, CloudError>
    where
        F: Fn(&ureq::Agent, &str) -> Result<ureq::Response, ureq::Error>,
    {
        let mut attempt: u64 = 0;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(CloudError::Cancelled);
            }
            let bearer = self.bearer()?;
            match make(&self.agent, &bearer) {
                Ok(resp) => return Ok(resp),
                Err(ureq::Error::Status(code, resp)) => {
                    attempt += 1;
                    if (code == 429 || code >= 500) && attempt <= 4 {
                        let wait = resp
                            .header("Retry-After")
                            .and_then(|v| v.trim().parse::<u64>().ok())
                            .unwrap_or(2 * attempt)
                            .min(60);
                        pause(cancel, wait)?;
                        continue;
                    }
                    let body = resp.into_string().unwrap_or_default();
                    return Err(status_error(code, &body));
                }
                Err(ureq::Error::Transport(t)) => {
                    attempt += 1;
                    if attempt <= 3 {
                        pause(cancel, 2 * attempt)?;
                        continue;
                    }
                    return Err(CloudError::Network(t.to_string()));
                }
            }
        }
    }

    /// Every file of the app folder, subfolders included. `on_found` gets the running count.
    pub fn list_files(
        &mut self,
        cancel: &AtomicBool,
        on_found: &mut dyn FnMut(usize),
    ) -> Result<Vec<RemoteFile>, CloudError> {
        let mut files = Vec::new();
        let mut url = LIST_URL;
        let mut body = json!({ "path": "", "recursive": true, "limit": 2000, "include_deleted": false }).to_string();
        loop {
            let (u, b) = (url, body.clone());
            let resp = self.send(cancel, |agent, bearer| {
                agent
                    .post(u)
                    .set("Authorization", bearer)
                    .set("Content-Type", "application/json")
                    .send_string(&b)
            })?;
            let v = parse_response(resp)?;
            for entry in v["entries"].as_array().into_iter().flatten() {
                if entry[".tag"].as_str() != Some("file") {
                    continue;
                }
                let (Some(id), Some(path)) = (entry["id"].as_str(), entry["path_display"].as_str()) else {
                    continue;
                };
                files.push(RemoteFile {
                    id: id.to_string(),
                    path: path.trim_start_matches('/').to_string(),
                    size: entry["size"].as_u64().unwrap_or(0),
                });
            }
            on_found(files.len());
            match (v["has_more"].as_bool(), v["cursor"].as_str()) {
                (Some(true), Some(cursor)) => {
                    url = LIST_CONTINUE_URL;
                    body = json!({ "cursor": cursor }).to_string();
                }
                _ => break,
            }
        }
        Ok(files)
    }

    /// Downloads one file into `part_path` (the caller renames it when the whole sync step is fine). `on_bytes` gets
    /// the number of bytes written so far. The size is checked at the end.
    pub fn download(
        &mut self,
        cancel: &AtomicBool,
        file: &RemoteFile,
        part_path: &Path,
        on_bytes: &mut dyn FnMut(u64),
    ) -> Result<(), CloudError> {
        let arg = json!({ "path": file.id }).to_string();
        let resp = self.send(cancel, |agent, bearer| {
            agent
                .post(DOWNLOAD_URL)
                .set("Authorization", bearer)
                .set("Dropbox-API-Arg", &arg)
                .send_bytes(&[])
        })?;
        let mut reader = resp.into_reader();
        let mut out = File::create(part_path)?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            if cancel.load(Ordering::SeqCst) {
                return Err(CloudError::Cancelled);
            }
            let n = reader.read(&mut buf).map_err(|e| CloudError::Network(e.to_string()))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            total += n as u64;
            on_bytes(total);
        }
        out.flush()?;
        if total != file.size {
            return Err(CloudError::Api(format!("incomplete download ({total} of {} bytes)", file.size)));
        }
        Ok(())
    }

    /// Tells Dropbox to forget this sign-in (best effort: the caller removes the local token anyway).
    pub fn revoke(&mut self) {
        let never = AtomicBool::new(false);
        let _ = self.send(&never, |agent, bearer| agent.post(REVOKE_URL).set("Authorization", bearer).send_bytes(&[]));
    }
}

/// Sleeps `secs`, waking up often to notice a cancel.
fn pause(cancel: &AtomicBool, secs: u64) -> Result<(), CloudError> {
    for _ in 0..secs * 10 {
        if cancel.load(Ordering::SeqCst) {
            return Err(CloudError::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

fn parse_response(resp: ureq::Response) -> Result<Value, CloudError> {
    let text = resp.into_string().map_err(|e| CloudError::Network(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| CloudError::Api(format!("unreadable answer: {e}")))
}

fn read_json(result: Result<ureq::Response, ureq::Error>) -> Result<Value, CloudError> {
    match result {
        Ok(resp) => parse_response(resp),
        Err(ureq::Error::Status(code, resp)) => {
            let body = resp.into_string().unwrap_or_default();
            Err(status_error(code, &body))
        }
        Err(ureq::Error::Transport(t)) => Err(CloudError::Network(t.to_string())),
    }
}

fn status_error(code: u16, body: &str) -> CloudError {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    if code == 401 || v["error"].as_str() == Some("invalid_grant") {
        return CloudError::AuthExpired;
    }
    let text = v["error_summary"]
        .as_str()
        .or_else(|| v["error_description"].as_str())
        .unwrap_or(body)
        .chars()
        .take(160)
        .collect::<String>();
    CloudError::Api(format!("HTTP {code}: {}", text.trim()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_errors_are_sorted() {
        assert_eq!(status_error(401, "{}"), CloudError::AuthExpired);
        assert_eq!(status_error(400, r#"{"error":"invalid_grant"}"#), CloudError::AuthExpired);
        assert_eq!(
            status_error(409, r#"{"error_summary":"path/not_found/.."}"#),
            CloudError::Api("HTTP 409: path/not_found/..".into())
        );
        assert_eq!(status_error(500, "oops"), CloudError::Api("HTTP 500: oops".into()));
    }
}
