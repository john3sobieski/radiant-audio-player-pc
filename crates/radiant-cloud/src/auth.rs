//! Sign-in: OAuth 2 + PKCE in the system browser, answer on a local page (`http://127.0.0.1:53682/`).

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::pkce::{challenge, new_state, new_verifier, percent_decode, percent_encode};
use crate::{api, clear_token, is_configured, load_token, redirect_uri, CloudError, APP_KEY, REDIRECT_PORT};

/// How long the browser page may stay unanswered.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// What the browser sent back to the local page.
#[derive(Debug, PartialEq, Eq)]
enum Callback {
    Code(String),
    Denied,
    /// Not our answer (favicon, a stray request, a wrong `state`).
    Ignore,
}

/// The address the browser is sent to.
pub(crate) fn authorize_url(challenge: &str, state: &str) -> String {
    format!(
        "https://www.dropbox.com/oauth2/authorize?client_id={}&response_type=code&redirect_uri={}\
         &code_challenge={}&code_challenge_method=S256&token_access_type=offline&state={}",
        percent_encode(APP_KEY),
        percent_encode(&redirect_uri()),
        challenge,
        state
    )
}

/// Reads `code` / `error` from a request target like `/?code=..&state=..`.
fn parse_callback(target: &str, state: &str) -> Callback {
    let Some((_, query)) = target.split_once('?') else { return Callback::Ignore };
    let (mut code, mut error, mut got_state) = (None, None, None);
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        match k {
            "code" => code = Some(percent_decode(v)),
            "error" => error = Some(percent_decode(v)),
            "state" => got_state = Some(percent_decode(v)),
            _ => {}
        }
    }
    if got_state.as_deref() != Some(state) {
        return Callback::Ignore;
    }
    match (code, error) {
        (Some(c), _) if !c.is_empty() => Callback::Code(c),
        (_, Some(_)) => Callback::Denied,
        _ => Callback::Ignore,
    }
}

fn open_browser(url: &str) -> bool {
    #[cfg(windows)]
    let started = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(not(windows))]
    let started = std::process::Command::new("xdg-open").arg(url).spawn();
    started.is_ok()
}

fn reply(stream: &mut TcpStream, status: &str, body: &str) {
    let text = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(text.as_bytes());
}

const PAGE: &str = "<!doctype html><meta charset=utf-8><title>Radiant</title>\
<body style=\"font-family:sans-serif;text-align:center;margin-top:20vh\">\
<h2>Radiant</h2><p>You can close this tab and return to the program.</p></body>";

/// Handles one browser connection. `Some` = the sign-in has an answer.
fn handle(mut stream: TcpStream, state: &str) -> Option<Callback> {
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let mut data = Vec::new();
    let mut buf = [0u8; 1024];
    while data.len() < 4096 && !data.windows(2).any(|w| w == b"\r\n") {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => data.extend_from_slice(&buf[..n]),
        }
    }
    let head = String::from_utf8_lossy(&data);
    let target = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/");
    match parse_callback(target, state) {
        Callback::Ignore => {
            reply(&mut stream, "404 Not Found", "");
            None
        }
        other => {
            reply(&mut stream, "200 OK", PAGE);
            Some(other)
        }
    }
}

/// Opens the browser, waits for the answer and returns the refresh token (the caller stores it).
/// Blocks for up to five minutes: run it on a worker thread; `cancel` stops the wait.
pub fn login(cancel: &AtomicBool) -> Result<String, CloudError> {
    if !is_configured() {
        return Err(CloudError::NotConfigured);
    }
    let listener = TcpListener::bind(("127.0.0.1", REDIRECT_PORT)).map_err(|_| CloudError::PortBusy)?;
    listener.set_nonblocking(true)?;
    let verifier = new_verifier().map_err(CloudError::Io)?;
    let state = new_state().map_err(CloudError::Io)?;
    if !open_browser(&authorize_url(&challenge(&verifier), &state)) {
        return Err(CloudError::Browser);
    }
    let deadline = Instant::now() + LOGIN_TIMEOUT;
    let code = loop {
        if cancel.load(Ordering::SeqCst) {
            return Err(CloudError::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(CloudError::Timeout);
        }
        match listener.accept() {
            Ok((stream, _)) => match handle(stream, &state) {
                Some(Callback::Code(code)) => break code,
                Some(Callback::Denied) => return Err(CloudError::Denied),
                _ => {}
            },
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.into()),
        }
    };
    drop(listener);
    let (refresh, _access, _secs) = api::exchange_code(&code, &verifier)?;
    Ok(refresh)
}

/// Signs out: Dropbox is told to forget the token (best effort), then the stored token is removed.
pub fn disconnect(dir: &Path) -> Result<(), CloudError> {
    if let Some(token) = load_token(dir) {
        if is_configured() {
            if let Ok(mut session) = api::Session::start(&token) {
                session.revoke();
            }
        }
    }
    clear_token(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_is_parsed() {
        assert_eq!(parse_callback("/?code=abc%2Fdef&state=S", "S"), Callback::Code("abc/def".into()));
        assert_eq!(parse_callback("/?error=access_denied&state=S", "S"), Callback::Denied);
        assert_eq!(parse_callback("/?code=abc&state=OTHER", "S"), Callback::Ignore);
        assert_eq!(parse_callback("/favicon.ico", "S"), Callback::Ignore);
        assert_eq!(parse_callback("/?state=S", "S"), Callback::Ignore);
    }

    #[test]
    fn url_has_the_pkce_parts() {
        let url = authorize_url("CH", "ST");
        assert!(url.starts_with("https://www.dropbox.com/oauth2/authorize?"));
        assert!(url.contains("code_challenge=CH&code_challenge_method=S256"));
        assert!(url.contains("token_access_type=offline"));
        assert!(url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A53682%2F"));
        assert!(url.ends_with("&state=ST"));
    }
}
