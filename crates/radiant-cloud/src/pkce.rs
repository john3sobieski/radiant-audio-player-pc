//! PKCE (RFC 7636) helpers and the small URL codecs the sign-in needs. No external crate except `sha2`.

use sha2::{Digest, Sha256};

const B64URL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64 with the URL-safe alphabet and no padding.
pub fn base64url(data: &[u8]) -> String {
    let mut out = String::with_capacity((data.len() * 4 + 2) / 3);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(B64URL[((n >> 18) & 63) as usize] as char);
        out.push(B64URL[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64URL[((n >> 6) & 63) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(B64URL[(n & 63) as usize] as char);
        }
    }
    out
}

/// `BASE64URL(SHA256(verifier))`
pub fn challenge(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64url(digest.as_slice())
}

/// 32 random bytes -> a 43-character verifier (RFC 7636 allows 43..=128).
pub fn new_verifier() -> Result<String, String> {
    Ok(base64url(&radiant_core::applock::random_bytes(32)?))
}

/// 16 random bytes for the `state` parameter (protects the local page from foreign requests).
pub fn new_state() -> Result<String, String> {
    Ok(base64url(&radiant_core::applock::random_bytes(16)?))
}

/// Percent-encodes everything except the unreserved characters.
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for &b in text.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Reverses [`percent_encode`] (and `+` = space, as in query strings). Invalid sequences stay as they are.
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() && hex(bytes[i + 1]).is_some() && hex(bytes[i + 2]).is_some() => {
                out.push(hex(bytes[i + 1]).unwrap_or(0) * 16 + hex(bytes[i + 2]).unwrap_or(0));
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_vectors() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn challenge_matches_rfc_7636_example() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_and_state_are_fresh_and_valid() {
        let a = new_verifier().unwrap();
        let b = new_verifier().unwrap();
        assert_eq!(a.len(), 43);
        assert_ne!(a, b);
        assert!(a.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'));
        assert_eq!(new_state().unwrap().len(), 22);
    }

    #[test]
    fn percent_round_trip() {
        let text = "http://127.0.0.1:53682/ a&b=c/д";
        assert_eq!(percent_decode(&percent_encode(text)), text);
        assert_eq!(percent_encode("a b"), "a%20b");
        assert_eq!(percent_decode("a+b%2Fc%zz"), "a b/c%zz");
    }
}
