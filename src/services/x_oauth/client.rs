use std::time::Duration;

use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use once_cell::sync::Lazy;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::config::{X_CLIENT_ID, X_CLIENT_SECRET, X_REDIRECT_URI};

const AUTHORIZE_URL: &str = "https://x.com/i/oauth2/authorize";
const TOKEN_URL: &str = "https://api.x.com/2/oauth2/token";
const API_BASE: &str = "https://api.x.com/2";
// Space-separated scopes, pre-encoded as %20 to avoid +/space ambiguity.
const SCOPES: &str = "users.read%20tweet.read%20offline.access";

static HTTP: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("failed to build reqwest client")
});

#[derive(Debug, Clone, Deserialize)]
pub struct XTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct XUserInfo {
    pub id: String,
    pub username: String,
    pub followers_count: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct XFollowCheck {
    pub is_following: bool,
    pub x_handle: String,
    pub x_image_uri: String,
    pub x_followers_count: i64,
    pub is_x_verified: bool,
}

/// PKCE: random verifier (64 url-safe chars, within the 43-128 range) and its
/// S256 challenge = base64url(sha256(verifier)).
pub fn generate_pkce() -> (String, String) {
    // NOTE: `rand::random::<[u8; 48]>()` doesn't compile here — rand 0.8's
    // blanket `Distribution<[T; N]>` impl only covers N<=32 without the
    // `min_const_gen` feature (not enabled in this project's Cargo.toml).
    // `fill_bytes` draws from the same thread-local CSPRNG and is equivalent.
    let mut bytes = [0u8; 48];
    rand::thread_rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

pub fn build_authorize_url(state: &str, code_challenge: &str) -> String {
    format!(
        "{AUTHORIZE_URL}?response_type=code&client_id={cid}&redirect_uri={ruri}\
&scope={scope}&state={state}&code_challenge={chal}&code_challenge_method=S256",
        cid = urlencoding::encode(&X_CLIENT_ID),
        ruri = urlencoding::encode(&X_REDIRECT_URI),
        scope = SCOPES,
        state = urlencoding::encode(state),
        chal = code_challenge, // already url-safe (no pad)
    )
}

/// Larger avatar: X returns a `_normal` (48px) image; `_400x400` is higher-res.
fn upscale_avatar(url: &str) -> String {
    url.replace("_normal", "_400x400")
}

pub(crate) fn parse_me(body: &str) -> Option<XUserInfo> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let data = v.get("data")?;
    let id = data.get("id")?.as_str()?.to_string();
    let username = data.get("username")?.as_str()?.to_string();
    let followers_count = data
        .get("public_metrics")
        .and_then(|m| m.get("followers_count"))
        .and_then(|n| n.as_i64())
        .unwrap_or(0);
    Some(XUserInfo {
        id,
        username,
        followers_count,
    })
}

pub(crate) fn parse_follow_check(body: &str) -> Option<XFollowCheck> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let data = v.get("data")?;
    let x_handle = data.get("username")?.as_str()?.to_string();
    let is_following = data
        .get("connection_status")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().any(|s| s.as_str() == Some("followed_by")))
        .unwrap_or(false);
    let x_image_uri = data
        .get("profile_image_url")
        .and_then(|s| s.as_str())
        .map(upscale_avatar)
        .unwrap_or_default();
    let x_followers_count = data
        .get("public_metrics")
        .and_then(|m| m.get("followers_count"))
        .and_then(|n| n.as_i64())
        .unwrap_or(0);
    // X Premium (Blue) accounts have `verified: false` but `verified_type: "blue"`
    // — reading the legacy `verified` field alone misses every modern checkmark.
    // Verified if legacy-verified OR any verified_type (blue/business/government).
    let is_x_verified = data
        .get("verified")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
        || data
            .get("verified_type")
            .and_then(|v| v.as_str())
            .map(|t| t != "none")
            .unwrap_or(false);
    Some(XFollowCheck {
        is_following,
        x_handle,
        x_image_uri,
        x_followers_count,
        is_x_verified,
    })
}

pub async fn exchange_code(code: &str, code_verifier: &str) -> Result<XTokenResponse> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", X_REDIRECT_URI.as_str()),
        ("code_verifier", code_verifier),
        ("client_id", X_CLIENT_ID.as_str()),
    ];
    // Public client relies on client_id in the body; confidential client ALSO
    // authenticates with HTTP Basic (client_id:client_secret).
    let mut req = HTTP.post(TOKEN_URL).form(&form);
    if !X_CLIENT_SECRET.is_empty() {
        req = req.basic_auth(X_CLIENT_ID.as_str(), Some(X_CLIENT_SECRET.as_str()));
    }
    let resp = req.send().await.context("x token request failed")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("x token endpoint status {}: {}", status, body);
    }
    serde_json::from_str(&body).context("decode x token response")
}

pub async fn get_me(access_token: &str) -> Result<XUserInfo> {
    let url = format!("{API_BASE}/users/me?user.fields=public_metrics");
    let resp = HTTP
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .context("x get_me request failed")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("x get_me status {}: {}", status, body);
    }
    parse_me(&body).context("parse x get_me response")
}

pub async fn check_follows_me(access_token: &str, target_handle: &str) -> Result<XFollowCheck> {
    let url = format!(
        "{API_BASE}/users/by/username/{}?user.fields=connection_status,profile_image_url,public_metrics,verified,verified_type",
        urlencoding::encode(target_handle)
    );
    let resp = HTTP
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .context("x follow-check request failed")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("x follow-check status {}: {}", status, body);
    }
    parse_follow_check(&body).context("parse x follow-check response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_s256_of_verifier() {
        let (verifier, challenge) = generate_pkce();
        assert!(verifier.len() >= 43 && verifier.len() <= 128);
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, expected);
    }

    #[test]
    fn parse_me_extracts_id_and_followers() {
        let body = r#"{"data":{"id":"123","username":"somehandle",
            "public_metrics":{"followers_count":128000,"following_count":10}}}"#;
        let me = parse_me(body).unwrap();
        assert_eq!(
            me,
            XUserInfo {
                id: "123".into(),
                username: "somehandle".into(),
                followers_count: 128000
            }
        );
        assert_eq!(me.username, "somehandle");
    }

    #[test]
    fn parse_follow_check_detects_followed_by() {
        let body = r#"{"data":{"connection_status":["followed_by","following"],
            "id":"1","username":"builnad",
            "profile_image_url":"https://pbs.twimg.com/x_normal.jpg",
            "public_metrics":{"followers_count":5000},"verified":true}}"#;
        let c = parse_follow_check(body).unwrap();
        assert!(c.is_following);
        assert_eq!(c.x_handle, "builnad");
        assert_eq!(c.x_image_uri, "https://pbs.twimg.com/x_400x400.jpg");
        assert_eq!(c.x_followers_count, 5000);
        assert_eq!(c.is_x_verified, true);
    }

    #[test]
    fn parse_follow_check_verified_via_verified_type_blue() {
        // X Premium (Blue): legacy `verified` is false but `verified_type` is
        // "blue" — must still resolve to is_x_verified = true (the real-world bug).
        let body = r#"{"data":{"connection_status":["followed_by"],"id":"1",
            "username":"bakbar8519","profile_image_url":"u_normal.jpg",
            "public_metrics":{"followers_count":5000},"verified":false,"verified_type":"blue"}}"#;
        let c = parse_follow_check(body).unwrap();
        assert_eq!(c.is_x_verified, true);
    }

    #[test]
    fn parse_follow_check_verified_type_none_is_false() {
        let body = r#"{"data":{"connection_status":["followed_by"],"id":"1",
            "username":"x","profile_image_url":"u_normal.jpg",
            "public_metrics":{"followers_count":5000},"verified":false,"verified_type":"none"}}"#;
        let c = parse_follow_check(body).unwrap();
        assert_eq!(c.is_x_verified, false);
    }

    #[test]
    fn parse_follow_check_false_when_not_followed() {
        let body = r#"{"data":{"connection_status":["following"],"id":"1",
            "username":"b","profile_image_url":"u_normal.jpg","public_metrics":{"followers_count":1}}}"#;
        let c = parse_follow_check(body).unwrap();
        assert!(!c.is_following);
        // `verified` absent from the response entirely → defaults to false.
        assert_eq!(c.is_x_verified, false);
    }

    #[test]
    fn parse_follow_check_explicit_verified_false() {
        // Distinguishes "field absent" from "field explicitly false" — both
        // must map to `false`, but this proves the field is actually being
        // read from the response, not just defaulted.
        let body = r#"{"data":{"connection_status":["followed_by"],"id":"1",
            "username":"b","profile_image_url":"u_normal.jpg",
            "public_metrics":{"followers_count":1},"verified":false}}"#;
        let c = parse_follow_check(body).unwrap();
        assert!(c.is_following);
        assert_eq!(c.is_x_verified, false);
    }
}
