//! Verify X's `x-twitter-webhooks-signature: sha256=<base64>` header against
//! HMAC-SHA256(consumer_secret, raw_request_body). Constant-time compare.
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Returns true iff `header_value` (e.g. "sha256=AbC...") matches the HMAC of
/// `raw_body` under `consumer_secret`.
pub fn verify(consumer_secret: &str, raw_body: &[u8], header_value: &str) -> bool {
    let Some(b64) = header_value.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = base64::engine::general_purpose::STANDARD.decode(b64) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(consumer_secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(raw_body);
    mac.verify_slice(&expected).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::gift::crc::response_token;

    #[test]
    fn valid_signature_passes() {
        let secret = "topsecret";
        let body = br#"{"tweet_create_events":[]}"#;
        let header = response_token(secret, std::str::from_utf8(body).unwrap());
        assert!(verify(secret, body, &header));
    }

    #[test]
    fn tampered_body_fails() {
        let secret = "topsecret";
        let header = response_token(secret, "original");
        assert!(!verify(secret, b"tampered", &header));
    }

    #[test]
    fn malformed_header_fails() {
        assert!(!verify("s", b"x", "not-prefixed"));
        assert!(!verify("s", b"x", "sha256=!!!notbase64"));
    }
}
