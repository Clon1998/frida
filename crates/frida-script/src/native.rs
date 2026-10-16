//! The native helpers behind `crypto`, `btoa`/`atob` and the `uuid` function. Everything else
//! is JavaScript (see `prelude.js`).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hmac::{EagerHash, Hmac, KeyInit, Mac};
use sha2::Digest;

/// `crypto.sha256(text)` and friends: the hex digest of the UTF-8 text.
pub fn hash(algorithm: &str, data: &str) -> Result<String, String> {
    let bytes = data.as_bytes();
    let digest = match algorithm {
        "md5" => md5::Md5::digest(bytes).to_vec(),
        "sha1" => sha1::Sha1::digest(bytes).to_vec(),
        "sha256" => sha2::Sha256::digest(bytes).to_vec(),
        "sha512" => sha2::Sha512::digest(bytes).to_vec(),
        other => return Err(format!("unknown hash algorithm {other:?}")),
    };
    Ok(hex(&digest))
}

/// `crypto.hmac(algorithm, key, data)`: hex HMAC of the UTF-8 data with the UTF-8 key.
pub fn hmac(algorithm: &str, key: &str, data: &str) -> Result<String, String> {
    // A small generic helper: `D` is the hash type, like a Java type parameter.
    fn mac<D: EagerHash>(key: &[u8], data: &[u8]) -> Vec<u8>
    where
        Hmac<D>: KeyInit + Mac,
    {
        let mut mac = <Hmac<D> as KeyInit>::new_from_slice(key).expect("HMAC accepts any key");
        mac.update(data);
        mac.finalize().into_bytes().to_vec()
    }
    let (key, data) = (key.as_bytes(), data.as_bytes());
    let digest = match algorithm {
        "md5" => mac::<md5::Md5>(key, data),
        "sha1" => mac::<sha1::Sha1>(key, data),
        "sha256" => mac::<sha2::Sha256>(key, data),
        "sha512" => mac::<sha2::Sha512>(key, data),
        other => return Err(format!("unknown HMAC algorithm {other:?}")),
    };
    Ok(hex(&digest))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Browser `btoa`: base64 of a "binary string" (every character must be below 256).
pub fn btoa(text: &str) -> Result<String, String> {
    let bytes: Option<Vec<u8>> = text.chars().map(|c| u8::try_from(c).ok()).collect();
    match bytes {
        Some(bytes) => Ok(STANDARD.encode(bytes)),
        None => Err("btoa: the string contains characters outside of Latin1".into()),
    }
}

/// Browser `atob`: decodes base64 into a "binary string".
pub fn atob(encoded: &str) -> Result<String, String> {
    let cleaned: String = encoded.chars().filter(|c| !c.is_whitespace()).collect();
    let bytes = STANDARD
        .decode(cleaned.as_bytes())
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(cleaned.as_bytes()))
        .map_err(|e| format!("atob: {e}"))?;
    Ok(bytes.into_iter().map(char::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_match_known_values() {
        assert_eq!(
            hash("md5", "abc").unwrap(),
            "900150983cd24fb0d6963f7d28e17f72"
        );
        assert_eq!(
            hash("sha1", "abc").unwrap(),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
        assert_eq!(
            hash("sha256", "abc").unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(hash("crc", "abc").is_err());
    }

    #[test]
    fn hmac_matches_rfc_4231() {
        // RFC 4231 test case 2.
        assert_eq!(
            hmac("sha256", "Jefe", "what do ya want for nothing?").unwrap(),
            "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn base64_round_trips_binary_strings() {
        assert_eq!(btoa("hello").unwrap(), "aGVsbG8=");
        assert_eq!(atob("aGVsbG8=").unwrap(), "hello");
        assert_eq!(atob("aGVsbG8").unwrap(), "hello");
        assert!(btoa("€").is_err());
    }
}
