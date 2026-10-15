//! Decoding (not verifying) JWT claims for the token viewer.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// The claims of a JWT as pretty JSON, or `None` if `token` isn't a JWT. The signature is not
/// checked: this is only for showing the user what's inside their token.
pub fn decode_claims(token: &str) -> Option<String> {
    let mut parts = token.split('.');
    let (_header, payload, _signature) = (parts.next()?, parts.next()?, parts.next()?);
    if parts.next().is_some() {
        return None;
    }
    let bytes = URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    serde_json::to_string_pretty(&value).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_payload_of_a_jwt() {
        // {"alg":"none"} . {"sub":"alice","scope":"orders/read"} . (empty signature)
        let token = "eyJhbGciOiJub25lIn0.eyJzdWIiOiJhbGljZSIsInNjb3BlIjoib3JkZXJzL3JlYWQifQ.";
        let claims = decode_claims(token).unwrap();
        assert!(claims.contains("\"sub\": \"alice\""));
        assert_eq!(decode_claims("opaque-token"), None);
    }
}
