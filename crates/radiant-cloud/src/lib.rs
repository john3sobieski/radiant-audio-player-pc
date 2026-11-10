//! radiant-cloud: Dropbox one-way download (PC counterpart of the Android `Cloud.kt`).
//!
//! * Sign-in: OAuth 2 with PKCE in the system browser, the answer comes back to a tiny local server on
//!   `127.0.0.1` (`auth.rs`). The app never sees the password; only a refresh token is kept (`token.rs`).
//! * Download: the whole app folder is listed and every missing music file is copied into
//!   `<music folder>/Dropbox` (`sync.rs`). Nothing is ever uploaded, changed or deleted in the cloud, and nothing
//!   local is deleted or overwritten.
//! * All calls block: run them on a worker thread, never on the UI thread.
//!
//! Setup in the Dropbox App Console (once, by the owner): app type "Scoped access", access "App folder",
//! permissions `files.metadata.read` and `files.content.read`, redirect URI `http://127.0.0.1:53682/`
//! (= [`redirect_uri`]), then paste the app key into [`APP_KEY`].

mod api;
mod auth;
mod pkce;
mod sync;
mod token;

pub use auth::{disconnect, login};
pub use sync::{local_path, sync, wanted, Progress, Summary};
pub use token::{clear_token, load_token, save_token};

use std::fmt;

/// The Dropbox app key. It is public by design (PKCE needs no secret). Paste yours between the quotes; while it is
/// empty the Dropbox card says that Dropbox is not set up in this build.
pub const APP_KEY: &str = "eslhya9s18jb2c8";

/// Port of the local sign-in page. It must be registered as a redirect URI in the App Console, so it is fixed.
pub const REDIRECT_PORT: u16 = 53682;

/// The name of the download folder inside the music folder.
pub const FOLDER_NAME: &str = "Dropbox";

/// `http://127.0.0.1:53682/`
pub fn redirect_uri() -> String {
    format!("http://127.0.0.1:{REDIRECT_PORT}/")
}

/// False while [`APP_KEY`] is empty.
pub fn is_configured() -> bool {
    !APP_KEY.trim().is_empty()
}

/// What can go wrong. The UI turns the variants into translated messages; `Display` is English (for logs and the
/// detail in "Dropbox error: ...").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CloudError {
    /// No app key in this build.
    NotConfigured,
    /// The local port of the sign-in page is taken by another program.
    PortBusy,
    /// The system browser could not be started.
    Browser,
    /// The user pressed "Deny" in the browser.
    Denied,
    /// Nobody answered in the browser in time.
    Timeout,
    Cancelled,
    /// There is no stored token.
    NotSignedIn,
    /// The token was revoked or expired: sign in again.
    AuthExpired,
    /// No music folder is chosen.
    NoFolder,
    Network(String),
    Api(String),
    Io(String),
}

impl fmt::Display for CloudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConfigured => write!(f, "no Dropbox app key in this build"),
            Self::PortBusy => write!(f, "port {REDIRECT_PORT} is busy"),
            Self::Browser => write!(f, "could not open the browser"),
            Self::Denied => write!(f, "access was denied"),
            Self::Timeout => write!(f, "sign-in took too long"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::NotSignedIn => write!(f, "not signed in"),
            Self::AuthExpired => write!(f, "access has expired"),
            Self::NoFolder => write!(f, "no music folder"),
            Self::Network(e) => write!(f, "network: {e}"),
            Self::Api(e) => write!(f, "{e}"),
            Self::Io(e) => write!(f, "file: {e}"),
        }
    }
}

impl std::error::Error for CloudError {}

impl From<std::io::Error> for CloudError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}
