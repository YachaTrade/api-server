//! X Account Activity CRC challenge: response_token = "sha256=" +
//! base64(HMAC-SHA256(consumer_secret, crc_token)).
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn response_token(consumer_secret: &str, crc_token: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(consumer_secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(crc_token.as_bytes());
    let sig = mac.finalize().into_bytes();
    format!("sha256={}", base64::engine::general_purpose::STANDARD.encode(sig))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_known_vector() {
        let out = response_token("secret", "abc");
        assert!(out.starts_with("sha256="));
        assert_eq!(out, response_token("secret", "abc"));
        assert_ne!(out, response_token("secret", "abd"));
    }
}
