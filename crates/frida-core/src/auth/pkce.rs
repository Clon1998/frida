//! PKCE (RFC 7636) and the `state` parameter for the authorization code flow.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

/// Characters allowed in a PKCE code verifier (RFC 7636 §4.1).
const VERIFIER_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

/// A random string of `len` verifier-safe characters.
pub fn random_string(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    getrandom::fill(&mut bytes).expect("the OS random number generator is unavailable");
    bytes
        .iter()
        .map(|b| VERIFIER_CHARS[*b as usize % VERIFIER_CHARS.len()] as char)
        .collect()
}

/// A new code verifier (64 characters; RFC 7636 allows 43 to 128).
pub fn code_verifier() -> String {
    random_string(64)
}

/// The S256 challenge for a verifier: `BASE64URL(SHA256(verifier))` without padding.
pub fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_matches_the_rfc_example() {
        // RFC 7636, Appendix B.
        assert_eq!(
            code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifiers_are_long_enough_and_random() {
        let v = code_verifier();
        assert_eq!(v.len(), 64);
        assert!(v.bytes().all(|b| VERIFIER_CHARS.contains(&b)));
        assert_ne!(v, code_verifier());
    }
}
