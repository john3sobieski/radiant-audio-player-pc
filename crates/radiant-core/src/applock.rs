//! App lock logic (README / PROGRESS stage 10). No UI here.
//!
//! Reference: `AppLock.kt` on Android. What this file does:
//! - three kinds of code: 4 digits, 6 digits, or a custom one (Latin letters and digits, 4..=64 chars);
//! - only a random salt and a PBKDF2-HMAC-SHA256 hash (30 000 iterations) are stored, never the code itself;
//! - hashes are compared in constant time;
//! - after every 5 wrong codes in a row the app locks for 30 s, 1, 2, 4, 8 min, then 15 min (the cap).
//!   The failure count and the unlock moment are saved to `applock.json`, so a restart does not reset them.
//!
//! No new crates: SHA-256, HMAC and PBKDF2 are written here (tested against the official vectors).
//! The random source is the OS one (`BCryptGenRandom` on Windows, `/dev/urandom` elsewhere).
//!
//! Recovery: a phrase of 12 random words (list of 700 in `wordlist.rs`, 114 bits). It is shown to the owner
//! once and saved as a text file; only its salted hash is stored (`recovery` in `applock.json`).
//! "Forgot the code" = enter the phrase (typed, or read from the saved file) -> the code is cleared of
//! its delay and the owner sets a new one. Trying phrases is NOT limited by the delay (the owner may be
//! locked out exactly then), and guessing is hopeless anyway.
//!
//! NOT here yet: the lock screen and the Security settings page.

use crate::settings::{replace_file, settings_path};
use crate::wordlist::{WORDS, WORD_COUNT};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const LOCK_FILE: &str = "applock.json";

pub const PBKDF2_ITERATIONS: u32 = 30_000;
pub const SALT_LEN: usize = 16;
pub const HASH_LEN: usize = 32;

/// Wrong codes in a row that trigger a delay.
pub const ATTEMPTS_PER_DELAY: u32 = 5;
/// First delay, seconds. Doubles after every further 5 failures.
pub const FIRST_DELAY_SECS: u64 = 30;
/// The delay never grows beyond this (15 minutes).
pub const MAX_DELAY_SECS: u64 = 15 * 60;

pub const CUSTOM_MIN_LEN: usize = 4;
pub const CUSTOM_MAX_LEN: usize = 64;

// ---------------------------------------------------------------------------------------------
// SHA-256, HMAC-SHA-256, PBKDF2
// ---------------------------------------------------------------------------------------------

const K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98,
    0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786,
    0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8,
    0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819,
    0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a,
    0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
    0xc67178f2,
];

const H0: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

fn compress(state: &mut [u32; 8], block: &[u8]) {
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

/// SHA-256 of `data`.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state = H0;
    let mut chunks = data.chunks_exact(64);
    for block in &mut chunks {
        compress(&mut state, block);
    }
    // Padding: 0x80, zeros, 64-bit big-endian bit length.
    let rest = chunks.remainder();
    let mut tail = [0u8; 128];
    tail[..rest.len()].copy_from_slice(rest);
    tail[rest.len()] = 0x80;
    let tail_len = if rest.len() < 56 { 64 } else { 128 };
    let bits = (data.len() as u64).wrapping_mul(8);
    tail[tail_len - 8..tail_len].copy_from_slice(&bits.to_be_bytes());
    for block in tail[..tail_len].chunks_exact(64) {
        compress(&mut state, block);
    }
    let mut out = [0u8; 32];
    for (i, v) in state.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
    }
    out
}

/// HMAC-SHA-256 (block size 64).
pub fn hmac_sha256(key: &[u8], msg: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        k[..32].copy_from_slice(&sha256(key));
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut inner = Vec::with_capacity(64 + msg.len());
    inner.extend(k.iter().map(|b| b ^ 0x36));
    inner.extend_from_slice(msg);
    let inner_hash = sha256(&inner);
    let mut outer = Vec::with_capacity(64 + 32);
    outer.extend(k.iter().map(|b| b ^ 0x5c));
    outer.extend_from_slice(&inner_hash);
    sha256(&outer)
}

/// PBKDF2-HMAC-SHA-256 with a 32-byte output (exactly one block).
pub fn pbkdf2_hmac_sha256(password: &[u8], salt: &[u8], iterations: u32) -> [u8; 32] {
    let mut first = Vec::with_capacity(salt.len() + 4);
    first.extend_from_slice(salt);
    first.extend_from_slice(&1u32.to_be_bytes());
    let mut u = hmac_sha256(password, &first);
    let mut t = u;
    for _ in 1..iterations.max(1) {
        u = hmac_sha256(password, &u);
        for (a, b) in t.iter_mut().zip(u.iter()) {
            *a ^= *b;
        }
    }
    t
}

/// Comparison whose time does not depend on where the first difference is.
pub fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 15) as usize] as char);
    }
    s
}

pub fn from_hex(text: &str) -> Option<Vec<u8>> {
    let t = text.as_bytes();
    if t.len() % 2 != 0 {
        return None;
    }
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    t.chunks_exact(2).map(|p| Some(digit(p[0])? << 4 | digit(p[1])?)).collect()
}

// ---------------------------------------------------------------------------------------------
// Random bytes from the operating system
// ---------------------------------------------------------------------------------------------

#[cfg(windows)]
pub fn random_bytes(len: usize) -> Result<Vec<u8>, String> {
    #[link(name = "bcrypt")]
    extern "system" {
        fn BCryptGenRandom(algorithm: *mut core::ffi::c_void, buffer: *mut u8, count: u32, flags: u32) -> i32;
    }
    const BCRYPT_USE_SYSTEM_PREFERRED_RNG: u32 = 0x0000_0002;
    let mut buf = vec![0u8; len];
    // SAFETY: `buf` is valid for `len` bytes; a null algorithm handle is allowed with the flag above.
    let status = unsafe {
        BCryptGenRandom(core::ptr::null_mut(), buf.as_mut_ptr(), len as u32, BCRYPT_USE_SYSTEM_PREFERRED_RNG)
    };
    if status == 0 {
        Ok(buf)
    } else {
        Err(format!("BCryptGenRandom failed: 0x{status:08x}"))
    }
}

#[cfg(not(windows))]
pub fn random_bytes(len: usize) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut buf = vec![0u8; len];
    fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut buf))
        .map_err(|e| format!("/dev/urandom: {e}"))?;
    Ok(buf)
}

// ---------------------------------------------------------------------------------------------
// Codes
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeKind {
    /// 4 digits.
    Pin4,
    /// 6 digits.
    Pin6,
    /// Latin letters and digits, 4..=64 characters.
    Custom,
}

impl CodeKind {
    /// Stable numbers for the UI (0 = 4 digits, 1 = 6 digits, 2 = custom).
    pub fn to_index(self) -> i32 {
        match self {
            CodeKind::Pin4 => 0,
            CodeKind::Pin6 => 1,
            CodeKind::Custom => 2,
        }
    }

    pub fn from_index(i: i32) -> Self {
        match i {
            1 => CodeKind::Pin6,
            2 => CodeKind::Custom,
            _ => CodeKind::Pin4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeError {
    /// A PIN must be exactly 4 or 6 digits.
    WrongLength,
    /// A PIN may contain digits only.
    NotDigits,
    /// A custom code is shorter than `CUSTOM_MIN_LEN` or longer than `CUSTOM_MAX_LEN`.
    CustomLength,
    /// A custom code may contain Latin letters and digits only.
    CustomChars,
}

/// Checks that a NEW code fits its kind. (A code typed at the lock screen is not validated, only compared.)
pub fn validate_code(kind: CodeKind, code: &str) -> Result<(), CodeError> {
    match kind {
        CodeKind::Pin4 | CodeKind::Pin6 => {
            let want = if kind == CodeKind::Pin4 { 4 } else { 6 };
            if !code.bytes().all(|b| b.is_ascii_digit()) {
                return Err(CodeError::NotDigits);
            }
            if code.len() != want {
                return Err(CodeError::WrongLength);
            }
            Ok(())
        }
        CodeKind::Custom => {
            if !code.bytes().all(|b| b.is_ascii_alphanumeric()) {
                return Err(CodeError::CustomChars);
            }
            if code.len() < CUSTOM_MIN_LEN || code.len() > CUSTOM_MAX_LEN {
                return Err(CodeError::CustomLength);
            }
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Stored secret: salt + hash
// ---------------------------------------------------------------------------------------------

/// A secret (the code, later the recovery phrase) as it is kept on disk: hex salt and hex hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HashRecord {
    pub salt: String,
    pub hash: String,
    #[serde(default = "default_iterations")]
    pub iterations: u32,
}

fn default_iterations() -> u32 {
    PBKDF2_ITERATIONS
}

impl HashRecord {
    /// New random salt, hash of `secret`.
    pub fn new(secret: &str) -> Result<Self, String> {
        let salt = random_bytes(SALT_LEN)?;
        Ok(Self::with_salt(secret, &salt))
    }

    fn with_salt(secret: &str, salt: &[u8]) -> Self {
        HashRecord {
            salt: to_hex(salt),
            hash: to_hex(&pbkdf2_hmac_sha256(secret.as_bytes(), salt, PBKDF2_ITERATIONS)),
            iterations: PBKDF2_ITERATIONS,
        }
    }

    /// Constant-time check. A damaged record (bad hex) never matches.
    pub fn verify(&self, secret: &str) -> bool {
        let (Some(salt), Some(want)) = (from_hex(&self.salt), from_hex(&self.hash)) else {
            return false;
        };
        if want.len() != HASH_LEN || self.iterations == 0 {
            return false;
        }
        let got = pbkdf2_hmac_sha256(secret.as_bytes(), &salt, self.iterations);
        ct_eq(&got, &want)
    }
}

// ---------------------------------------------------------------------------------------------
// Lock state
// ---------------------------------------------------------------------------------------------

/// Seconds to wait after `failures` wrong codes in a row: 0 unless `failures` is a multiple of 5,
/// then 30 s, 60, 120, 240, 480, 900 (cap).
pub fn delay_for_failures(failures: u32) -> u64 {
    if failures == 0 || failures % ATTEMPTS_PER_DELAY != 0 {
        return 0;
    }
    let step = (failures / ATTEMPTS_PER_DELAY - 1).min(16);
    (FIRST_DELAY_SECS << step).min(MAX_DELAY_SECS)
}

/// Current time, seconds since 1970 (0 if the clock is before that).
pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnlockResult {
    /// Correct code. The failure counter is reset.
    Ok,
    /// Wrong code. `left` = tries until the next delay starts (1..=5).
    Wrong { left: u32 },
    /// Wrong code, and this was the one that started a delay.
    WrongLocked { secs: u64 },
    /// Still waiting; the code was NOT checked and does not count as a failure.
    Locked { secs: u64 },
    /// No code is set (or the file is damaged): nothing to unlock.
    NotSet,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LockStore {
    pub kind: Option<CodeKind>,
    pub code: Option<HashRecord>,
    /// Hash of the normalized recovery phrase (`None` = no phrase was made).
    pub recovery: Option<HashRecord>,
    /// Wrong codes in a row (survives restarts).
    pub failures: u32,
    /// Unix time (seconds) when the current delay ends; 0 = no delay.
    pub locked_until: u64,
}

impl LockStore {
    /// The lock is on when a code with a readable record exists.
    pub fn is_enabled(&self) -> bool {
        self.kind.is_some() && self.code.is_some()
    }

    /// Sets a new code (replaces the old one, clears failures and delay). Does not touch `recovery`.
    pub fn set_code(&mut self, kind: CodeKind, code: &str) -> Result<(), String> {
        validate_code(kind, code).map_err(|e| format!("invalid code: {e:?}"))?;
        self.code = Some(HashRecord::new(code)?);
        self.kind = Some(kind);
        self.reset_failures();
        Ok(())
    }

    /// Turns the lock off completely (code, recovery, failures).
    pub fn clear(&mut self) {
        *self = LockStore::default();
    }

    pub fn reset_failures(&mut self) {
        self.failures = 0;
        self.locked_until = 0;
    }

    /// Seconds left of the current delay (0 = may try now).
    pub fn lock_remaining(&self, now: u64) -> u64 {
        // If the clock was moved far back, never wait longer than the biggest delay.
        self.locked_until.saturating_sub(now).min(MAX_DELAY_SECS)
    }

    /// Checks a code typed at the lock screen and updates the counters. The caller MUST save the store
    /// after every call (a wrong try has to survive a crash or a restart).
    pub fn try_unlock(&mut self, code: &str, now: u64) -> UnlockResult {
        let Some(record) = self.code.as_ref().filter(|_| self.kind.is_some()) else {
            return UnlockResult::NotSet;
        };
        let wait = self.lock_remaining(now);
        if wait > 0 {
            return UnlockResult::Locked { secs: wait };
        }
        if record.verify(code) {
            self.reset_failures();
            return UnlockResult::Ok;
        }
        self.failures = self.failures.saturating_add(1);
        let delay = delay_for_failures(self.failures);
        if delay > 0 {
            self.locked_until = now.saturating_add(delay);
            UnlockResult::WrongLocked { secs: delay }
        } else {
            self.locked_until = 0;
            UnlockResult::Wrong { left: ATTEMPTS_PER_DELAY - self.failures % ATTEMPTS_PER_DELAY }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Recovery phrase
// ---------------------------------------------------------------------------------------------

pub const PHRASE_WORDS: usize = 12;

/// A fresh random phrase: 12 words joined by single spaces. Words are drawn uniformly (rejection
/// sampling, so no word is more likely than another).
pub fn generate_phrase() -> Result<String, String> {
    // Largest multiple of WORD_COUNT that fits in 16 bits; values above it are thrown away.
    const LIMIT: u32 = 65_536 - 65_536 % WORD_COUNT as u32;
    let mut picked: Vec<&str> = Vec::with_capacity(PHRASE_WORDS);
    while picked.len() < PHRASE_WORDS {
        let bytes = random_bytes(2 * PHRASE_WORDS)?;
        for pair in bytes.chunks_exact(2) {
            let v = u32::from(u16::from_be_bytes([pair[0], pair[1]]));
            if v < LIMIT && picked.len() < PHRASE_WORDS {
                picked.push(WORDS[(v % WORD_COUNT as u32) as usize]);
            }
        }
    }
    Ok(picked.join(" "))
}

/// Turns what the owner typed into the canonical phrase: any case, any spaces / line breaks / commas
/// between words. `None` unless there are exactly 12 words and all of them are in the list.
pub fn normalize_phrase(text: &str) -> Option<String> {
    let words: Vec<String> = text
        .split(|c: char| c.is_whitespace() || c == ',' || c == ';' || c == '.')
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
        .collect();
    if words.len() != PHRASE_WORDS || !words.iter().all(|w| WORDS.binary_search(&w.as_str()).is_ok()) {
        return None;
    }
    Some(words.join(" "))
}

/// Text of the recovery file the owner saves (a short note, then the phrase on its own line).
pub fn recovery_file_text(phrase: &str) -> String {
    format!(
        "# Radiant - recovery phrase\n\
         # Keep this file somewhere safe and private. Anyone who has it can reset your app lock.\n\
         # Radiant stores only a hash of it and cannot show it to you again.\n\
         \n\
         {phrase}\n"
    )
}

/// Reads a phrase from the text of a recovery file: lines starting with `#` are ignored.
pub fn phrase_from_file_text(text: &str) -> Option<String> {
    let body: Vec<&str> = text.lines().filter(|l| !l.trim_start().starts_with('#')).collect();
    normalize_phrase(&body.join(" "))
}

/// Writes the recovery file (creates the folder if needed).
pub fn write_recovery_file(path: &Path, phrase: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
    }
    fs::write(path, recovery_file_text(phrase)).map_err(|e| format!("{}: {e}", path.display()))
}

/// Reads a recovery file. `None` = unreadable or it does not contain a valid phrase.
pub fn read_recovery_file(path: &Path) -> Option<String> {
    phrase_from_file_text(&fs::read_to_string(path).ok()?)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoverResult {
    /// The phrase is right: failures and delay are reset. The owner now sets a new code (`set_code`)
    /// or turns the lock off (`clear`).
    Ok,
    /// Not a valid phrase, or not this one.
    Wrong,
    /// No recovery phrase was ever made.
    NotSet,
}

impl LockStore {
    /// Makes a new recovery phrase, stores its hash (replacing an older one) and returns the phrase.
    /// The caller shows it / saves the file and then saves the store.
    pub fn create_recovery(&mut self) -> Result<String, String> {
        let phrase = generate_phrase()?;
        self.recovery = Some(HashRecord::new(&phrase)?);
        Ok(phrase)
    }

    pub fn has_recovery(&self) -> bool {
        self.recovery.is_some()
    }

    /// "Forgot the code": checks the typed phrase (or the text of the recovery file). On success the delay
    /// and the failure counter are cleared. The caller saves the store.
    pub fn recover(&mut self, typed: &str) -> RecoverResult {
        let Some(record) = self.recovery.as_ref() else { return RecoverResult::NotSet };
        match normalize_phrase(typed).or_else(|| phrase_from_file_text(typed)) {
            Some(p) if record.verify(&p) => {
                self.reset_failures();
                RecoverResult::Ok
            }
            _ => RecoverResult::Wrong,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// File
// ---------------------------------------------------------------------------------------------

pub fn lock_path() -> PathBuf {
    settings_path().with_file_name(LOCK_FILE)
}

pub fn load() -> Result<LockStore, String> {
    read_at(&lock_path())
}

pub fn save(store: &LockStore) -> Result<(), String> {
    write_at(&lock_path(), store)
}

fn read_at(path: &Path) -> Result<LockStore, String> {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str::<LockStore>(&text).map_err(|e| {
            // Keep a copy: the next save would overwrite the broken file.
            let _ = fs::copy(path, path.with_extension("json.bad"));
            format!("{}: {e}", path.display())
        }),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(LockStore::default()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn write_at(path: &Path, store: &LockStore) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        if !dir.as_os_str().is_empty() {
            fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
    }
    let mut data = serde_json::to_vec_pretty(store).map_err(|e| e.to_string())?;
    data.push(b'\n');
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, &data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    replace_file(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn sha256_vectors() {
        assert_eq!(to_hex(&sha256(b"")), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(to_hex(&sha256(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        // 56 bytes: the padding needs a second block.
        assert_eq!(
            to_hex(&sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        let million_a = vec![b'a'; 1_000_000];
        assert_eq!(
            to_hex(&sha256(&million_a)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn hmac_vectors() {
        // RFC 4231, test case 2.
        assert_eq!(
            to_hex(&hmac_sha256(b"Jefe", b"what do ya want for nothing?")),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
        // RFC 4231, test case 6: key longer than the block size.
        assert_eq!(
            to_hex(&hmac_sha256(&[0xaa; 131], b"Test Using Larger Than Block-Size Key - Hash Key First")),
            "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54"
        );
    }

    #[test]
    fn pbkdf2_vectors() {
        assert_eq!(
            to_hex(&pbkdf2_hmac_sha256(b"password", b"salt", 1)),
            "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
        );
        assert_eq!(
            to_hex(&pbkdf2_hmac_sha256(b"password", b"salt", 4096)),
            "c5e478d59288c841aa530db6845c4c8d962893a001ce4e11a4963873aa98134a"
        );
    }

    #[test]
    fn hex_round_trip() {
        let bytes = [0u8, 1, 0x7f, 0x80, 0xff];
        assert_eq!(from_hex(&to_hex(&bytes)).unwrap(), bytes);
        assert!(from_hex("abc").is_none());
        assert!(from_hex("zz").is_none());
        assert_eq!(from_hex("AB").unwrap(), vec![0xab]);
    }

    #[test]
    fn constant_time_compare() {
        assert!(ct_eq(b"abc", b"abc"));
        assert!(!ct_eq(b"abc", b"abd"));
        assert!(!ct_eq(b"abc", b"ab"));
        assert!(ct_eq(b"", b""));
    }

    #[test]
    fn random_bytes_differ() {
        let a = random_bytes(16).unwrap();
        let b = random_bytes(16).unwrap();
        assert_eq!(a.len(), 16);
        assert_ne!(a, b);
    }

    #[test]
    fn code_rules() {
        assert_eq!(validate_code(CodeKind::Pin4, "1234"), Ok(()));
        assert_eq!(validate_code(CodeKind::Pin4, "123"), Err(CodeError::WrongLength));
        assert_eq!(validate_code(CodeKind::Pin4, "12345"), Err(CodeError::WrongLength));
        assert_eq!(validate_code(CodeKind::Pin4, "12a4"), Err(CodeError::NotDigits));
        assert_eq!(validate_code(CodeKind::Pin6, "123456"), Ok(()));
        assert_eq!(validate_code(CodeKind::Pin6, "1234"), Err(CodeError::WrongLength));
        assert_eq!(validate_code(CodeKind::Custom, "abc1"), Ok(()));
        assert_eq!(validate_code(CodeKind::Custom, "abc"), Err(CodeError::CustomLength));
        assert_eq!(validate_code(CodeKind::Custom, &"a".repeat(64)), Ok(()));
        assert_eq!(validate_code(CodeKind::Custom, &"a".repeat(65)), Err(CodeError::CustomLength));
        assert_eq!(validate_code(CodeKind::Custom, "ab cd"), Err(CodeError::CustomChars));
        assert_eq!(validate_code(CodeKind::Custom, "пароль12"), Err(CodeError::CustomChars));
    }

    #[test]
    fn delays_grow_and_cap() {
        let d: Vec<u64> = (1..=40).map(delay_for_failures).collect();
        assert_eq!(&d[0..4], &[0, 0, 0, 0]);
        assert_eq!(d[4], 30);
        assert_eq!(d[5], 0);
        assert_eq!(d[9], 60);
        assert_eq!(d[14], 120);
        assert_eq!(d[19], 240);
        assert_eq!(d[24], 480);
        assert_eq!(d[29], 900);
        assert_eq!(d[34], 900);
        assert_eq!(d[39], 900);
        assert_eq!(delay_for_failures(0), 0);
        assert_eq!(delay_for_failures(u32::MAX - u32::MAX % 5), 900);
    }

    #[test]
    fn record_verifies_only_the_right_secret() {
        let r = HashRecord::new("1234").unwrap();
        assert!(r.verify("1234"));
        assert!(!r.verify("1235"));
        assert!(!r.verify(""));
        // The secret is not stored in any form.
        assert!(!r.salt.contains("1234") && !r.hash.contains("1234"));
        // Two records of the same secret have different salts.
        assert_ne!(HashRecord::new("1234").unwrap().salt, r.salt);
        let mut broken = r.clone();
        broken.hash = "zz".into();
        assert!(!broken.verify("1234"));
    }

    fn store_with(kind: CodeKind, code: &str) -> LockStore {
        let mut s = LockStore::default();
        assert!(!s.is_enabled());
        s.set_code(kind, code).unwrap();
        assert!(s.is_enabled());
        s
    }

    #[test]
    fn set_code_rejects_bad_codes() {
        let mut s = LockStore::default();
        assert!(s.set_code(CodeKind::Pin4, "12").is_err());
        assert!(!s.is_enabled());
    }

    #[test]
    fn five_failures_lock_and_success_resets() {
        let mut s = store_with(CodeKind::Pin4, "4321");
        let t = 1_000_000;
        assert_eq!(s.try_unlock("0000", t), UnlockResult::Wrong { left: 4 });
        assert_eq!(s.try_unlock("0000", t), UnlockResult::Wrong { left: 3 });
        // A correct code in between resets the streak.
        assert_eq!(s.try_unlock("4321", t), UnlockResult::Ok);
        assert_eq!(s.failures, 0);
        for left in (1..=4).rev() {
            assert_eq!(s.try_unlock("0000", t), UnlockResult::Wrong { left });
        }
        assert_eq!(s.try_unlock("0000", t), UnlockResult::WrongLocked { secs: 30 });
        // While waiting, even the right code is refused and nothing is counted.
        assert_eq!(s.try_unlock("4321", t + 10), UnlockResult::Locked { secs: 20 });
        assert_eq!(s.try_unlock("0000", t + 10), UnlockResult::Locked { secs: 20 });
        assert_eq!(s.failures, 5);
        // After the wait the right code works.
        assert_eq!(s.try_unlock("4321", t + 30), UnlockResult::Ok);
        assert_eq!(s.failures, 0);
        assert_eq!(s.locked_until, 0);
    }

    #[test]
    fn second_delay_is_a_minute() {
        let mut s = store_with(CodeKind::Custom, "Secret99");
        let mut t = 5_000;
        for _ in 0..4 {
            s.try_unlock("bad", t);
        }
        assert_eq!(s.try_unlock("bad", t), UnlockResult::WrongLocked { secs: 30 });
        t += 30;
        for left in (1..=4).rev() {
            assert_eq!(s.try_unlock("bad", t), UnlockResult::Wrong { left });
        }
        assert_eq!(s.try_unlock("bad", t), UnlockResult::WrongLocked { secs: 60 });
        assert_eq!(s.lock_remaining(t), 60);
        assert_eq!(s.lock_remaining(t + 61), 0);
    }

    #[test]
    fn clock_moved_back_never_waits_longer_than_the_cap() {
        let mut s = store_with(CodeKind::Pin6, "123456");
        s.locked_until = 10_000_000;
        assert_eq!(s.lock_remaining(1_000), MAX_DELAY_SECS);
    }

    #[test]
    fn not_set_and_clear() {
        let mut s = LockStore::default();
        assert_eq!(s.try_unlock("1234", 0), UnlockResult::NotSet);
        s.set_code(CodeKind::Pin4, "1234").unwrap();
        s.failures = 3;
        s.clear();
        assert!(!s.is_enabled());
        assert_eq!(s, LockStore::default());
    }

    #[test]
    fn kind_indexes_round_trip() {
        for k in [CodeKind::Pin4, CodeKind::Pin6, CodeKind::Custom] {
            assert_eq!(CodeKind::from_index(k.to_index()), k);
        }
        assert_eq!(CodeKind::from_index(99), CodeKind::Pin4);
    }

    #[test]
    fn file_round_trip_keeps_failures_and_delay() {
        let dir = env::temp_dir().join(format!("radiant-applock-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("applock.json");

        // No file = lock off.
        assert_eq!(read_at(&path).unwrap(), LockStore::default());

        let mut s = store_with(CodeKind::Pin4, "1111");
        for _ in 0..5 {
            s.try_unlock("2222", 777);
        }
        write_at(&path, &s).unwrap();
        let back = read_at(&path).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.failures, 5);
        assert_eq!(back.locked_until, 807);
        assert!(!path.with_extension("json.tmp").exists());
        // The file holds no plain code.
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("1111"));

        // Broken file: error + a `.bad` copy.
        fs::write(&path, "{ not json").unwrap();
        assert!(read_at(&path).is_err());
        assert!(path.with_extension("json.bad").exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn word_list_is_clean() {
        assert_eq!(WORDS.len(), WORD_COUNT);
        assert_eq!(WORD_COUNT, 700);
        assert!(WORDS.windows(2).all(|p| p[0] < p[1]), "sorted and without repeats");
        for w in WORDS.iter() {
            assert!((3..=9).contains(&w.len()), "{w}");
            assert!(w.bytes().all(|b| b.is_ascii_lowercase()), "{w}");
        }
    }

    #[test]
    fn generated_phrase_is_valid_and_random() {
        let a = generate_phrase().unwrap();
        let b = generate_phrase().unwrap();
        assert_eq!(a.split(' ').count(), PHRASE_WORDS);
        assert_eq!(normalize_phrase(&a).as_deref(), Some(a.as_str()));
        assert_ne!(a, b);
    }

    #[test]
    fn phrase_input_is_forgiving_but_strict_about_words() {
        let p = generate_phrase().unwrap();
        let messy = format!("  {}\n", p.to_uppercase().replace(' ', " \n  "));
        assert_eq!(normalize_phrase(&messy), Some(p.clone()));
        assert_eq!(normalize_phrase(&p.replace(' ', ", ")), Some(p.clone()));
        // 11 words, 13 words, a word that is not in the list.
        let words: Vec<&str> = p.split(' ').collect();
        assert!(normalize_phrase(&words[..11].join(" ")).is_none());
        assert!(normalize_phrase(&format!("{p} {}", words[0])).is_none());
        assert!(normalize_phrase(&format!("{} zzzzzz", words[..11].join(" "))).is_none());
        assert!(normalize_phrase("").is_none());
    }

    #[test]
    fn recovery_file_round_trip() {
        let p = generate_phrase().unwrap();
        let text = recovery_file_text(&p);
        assert!(text.starts_with('#'));
        assert_eq!(phrase_from_file_text(&text), Some(p.clone()));
        assert!(phrase_from_file_text("# only a comment\n").is_none());

        let dir = env::temp_dir().join(format!("radiant-recovery-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("sub").join("Radiant recovery.txt");
        write_recovery_file(&path, &p).unwrap();
        assert_eq!(read_recovery_file(&path), Some(p));
        assert_eq!(read_recovery_file(&dir.join("missing.txt")), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn recovery_resets_the_delay_and_keeps_working_after_a_new_code() {
        let mut s = store_with(CodeKind::Pin4, "1234");
        assert_eq!(s.recover("anything"), RecoverResult::NotSet);
        assert!(!s.has_recovery());
        let phrase = s.create_recovery().unwrap();
        assert!(s.has_recovery());
        for _ in 0..5 {
            s.try_unlock("0000", 100);
        }
        assert!(s.lock_remaining(101) > 0);

        let words: Vec<&str> = phrase.split(' ').collect();
        assert_eq!(s.recover(&words[..11].join(" ")), RecoverResult::Wrong);
        assert_eq!(s.recover(&generate_phrase().unwrap()), RecoverResult::Wrong);
        assert!(s.lock_remaining(101) > 0, "a wrong phrase changes nothing");

        assert_eq!(s.recover(&phrase.to_uppercase()), RecoverResult::Ok);
        assert_eq!(s.lock_remaining(101), 0);
        assert_eq!(s.failures, 0);

        // A new code keeps the same phrase valid; clear() forgets everything.
        s.set_code(CodeKind::Pin6, "654321").unwrap();
        assert_eq!(s.try_unlock("654321", 200), UnlockResult::Ok);
        assert_eq!(s.recover(&recovery_file_text(&phrase)), RecoverResult::Ok);
        s.clear();
        assert_eq!(s.recover(&phrase), RecoverResult::NotSet);
    }
}
