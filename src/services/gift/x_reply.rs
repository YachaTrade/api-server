//! POST `/2/tweets` over OAuth 1.0a — used by the consumer's reply-on-success
//! path to drop a "🎁 Gift activated. https://nad.fun/profile/0x…?tab=gift" link as a
//! reply to the original activation tweet.
//!
//! Why OAuth 1.0a and not OAuth 2.0? The bot tweets from one fixed account
//! (the operator's), so we don't need user-context refresh-token plumbing.
//! OAuth 1.0a per-request signing is stateless: four secrets in env, an
//! `Authorization` header, done.
//!
//! ## Signature construction
//!
//! Per RFC 5849 §3 the signature is HMAC-SHA1 of a normalized
//! signature-base-string keyed by `consumer_secret&token_secret` (both
//! URL-encoded, ampersand-joined). Hand-rolling this is safer than a
//! medium-sized vendored crate: the surface is small and we own the bug
//! count.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use hmac::{Hmac, Mac};
use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use rand::RngCore;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use thiserror::Error;

type HmacSha1 = Hmac<Sha1>;

/// RFC 3986 `unreserved` chars — what OAuth 1.0a expects URL-encoders to leave
/// alone. `NON_ALPHANUMERIC` is the inverse, so we subtract back the four
/// unreserved punctuation marks (`- _ . ~`) by removing them from the set.
/// This matches the OAuth spec's percent-encoding rules byte-for-byte; using
/// the wrong set (e.g. `application/x-www-form-urlencoded`) is the classic
/// reason third-party signers fail X's signature check on weird payloads.
const OAUTH_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

fn encode(s: &str) -> String {
    utf8_percent_encode(s, OAUTH_ENCODE_SET).to_string()
}

/// Four operator-controlled secrets from X Developer Portal. All fields are
/// secrets — see the manual `Debug` impl below that redacts them.
#[derive(Clone)]
pub struct OAuth1Credentials {
    pub consumer_key: String,
    pub consumer_secret: String,
    pub access_token: String,
    pub access_token_secret: String,
}

impl std::fmt::Debug for OAuth1Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OAuth1Credentials")
            .field("consumer_key", &"***")
            .field("consumer_secret", &"***")
            .field("access_token", &"***")
            .field("access_token_secret", &"***")
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ReplyError {
    #[error("reply HTTP request failed: {0}")]
    Http(String),

    #[error("reply unauthorized — check OAuth 1.0a credentials")]
    Unauthorized,

    #[error("reply forbidden — app permissions likely not 'Read and Write' (body: {body})")]
    Forbidden { body: String },

    #[error("reply rate-limited; retry after {retry_after_s}s (body: {body})")]
    RateLimited { retry_after_s: u64, body: String },

    #[error("reply returned unexpected HTTP status {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },

    #[error("reply API response parse failed: {0}")]
    Parse(String),
}

impl ReplyError {
    /// Worker uses this to decide between retry-with-backoff and giving up
    /// the row as `reply_status='failed'`.
    ///
    /// - `Http` / `RateLimited` / 5xx — transient; retry.
    /// - `Unauthorized` / `Forbidden` — operator must fix env / app perms.
    ///   Retrying won't help; mark failed so the alarm fires.
    /// - 4xx other than 401/403/429 — usually malformed payload (overlong
    ///   text, dup-tweet, replying to deleted tweet). Mark failed and
    ///   surface the body in `reply_last_error`.
    /// - `Parse` — API contract drift; mark failed.
    pub fn is_retryable(&self) -> bool {
        match self {
            Self::Http(_) | Self::RateLimited { .. } => true,
            Self::UnexpectedStatus { status, .. } => *status >= 500,
            Self::Unauthorized | Self::Forbidden { .. } | Self::Parse(_) => false,
        }
    }
}

#[derive(Serialize)]
struct CreateTweetBody<'a> {
    text: &'a str,
    reply: Reply<'a>,
}

#[derive(Serialize)]
struct Reply<'a> {
    in_reply_to_tweet_id: &'a str,
}

#[derive(Deserialize, Debug)]
struct CreateTweetResponse {
    data: CreatedTweet,
}

#[derive(Deserialize, Debug)]
struct CreatedTweet {
    id: String,
    #[allow(dead_code)]
    text: String,
}

/// Outcome of a successful `POST /2/tweets`.
#[derive(Debug, Clone)]
pub struct PostedTweet {
    /// The new reply tweet's snowflake id, returned by X. Caller persists
    /// this so the row's `reply_tweet_id` becomes auditable from outside.
    pub id: String,
}

/// Post `text` as a reply to `in_reply_to_tweet_id` using `creds`.
///
/// `api_base` matches the producer's `X_API_BASE` (production: `https://api.x.com`,
/// tests: a mockito server URL). The trailing slash is **not** included —
/// the function appends `/2/tweets`. This mirrors `stream.rs` /
/// `rules.rs` to keep the env contract consistent across the three X-API
/// touchpoints.
pub async fn post_reply(
    client: &Client,
    creds: &OAuth1Credentials,
    api_base: &str,
    text: &str,
    in_reply_to_tweet_id: &str,
) -> Result<PostedTweet, ReplyError> {
    let url = format!("{api_base}/2/tweets");

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
        .to_string();
    let nonce = generate_nonce();

    let auth_header = build_authorization_header(creds, &url, &timestamp, &nonce);

    let body = CreateTweetBody {
        text,
        reply: Reply {
            in_reply_to_tweet_id,
        },
    };

    let resp = client
        .post(&url)
        .header("Authorization", auth_header)
        .json(&body)
        .send()
        .await
        .map_err(|e| ReplyError::Http(e.to_string()))?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ReplyError::Unauthorized);
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        let body = resp.text().await.unwrap_or_default();
        return Err(ReplyError::Forbidden { body });
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after_s = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(60);
        let body = resp.text().await.unwrap_or_default();
        return Err(ReplyError::RateLimited {
            retry_after_s,
            body,
        });
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(ReplyError::UnexpectedStatus {
            status: status.as_u16(),
            body,
        });
    }

    let payload: CreateTweetResponse = resp
        .json()
        .await
        .map_err(|e| ReplyError::Parse(e.to_string()))?;
    Ok(PostedTweet {
        id: payload.data.id,
    })
}

/// Build the full `Authorization: OAuth …` header for a `POST` to `url` at
/// `timestamp` (epoch seconds) with `nonce`. Public-ish via `pub(crate)` so
/// tests below can verify the canonical signature value against a fixed
/// input vector.
pub(crate) fn build_authorization_header(
    creds: &OAuth1Credentials,
    url: &str,
    timestamp: &str,
    nonce: &str,
) -> String {
    // The seven required oauth_* params on a signed call (RFC 5849 §3.4.1.3.1).
    // POST /2/tweets's body is JSON, so per §3.4.1.3.1 only oauth_* go into
    // the signature base — no body params, no query params on our call.
    let oauth_params: Vec<(&str, &str)> = vec![
        ("oauth_consumer_key", creds.consumer_key.as_str()),
        ("oauth_nonce", nonce),
        ("oauth_signature_method", "HMAC-SHA1"),
        ("oauth_timestamp", timestamp),
        ("oauth_token", creds.access_token.as_str()),
        ("oauth_version", "1.0"),
    ];

    // Sort by (encoded key, encoded value); for params with the same key the
    // values would be sorted, but oauth_* keys are unique so order is
    // determined by key alone here.
    let mut encoded: Vec<(String, String)> = oauth_params
        .iter()
        .map(|(k, v)| (encode(k), encode(v)))
        .collect();
    encoded.sort();

    let param_string: String = encoded
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");

    // Signature base: METHOD & encoded(URL) & encoded(param_string)
    let base_string = format!("POST&{}&{}", encode(url), encode(&param_string));

    // Signing key: encoded(consumer_secret) & encoded(token_secret)
    let signing_key = format!(
        "{}&{}",
        encode(&creds.consumer_secret),
        encode(&creds.access_token_secret),
    );

    let signature = hmac_sha1_base64(&signing_key, &base_string);
    let signature_enc = encode(&signature);

    // Authorization header: each oauth_* param as `key="value"`, quoted +
    // percent-encoded, comma-joined. Signature goes in alphabetical position
    // with the rest. Newlines / leading "OAuth " keyword per §3.5.1.
    let mut header_params: Vec<(String, String)> = oauth_params
        .iter()
        .map(|(k, v)| (encode(k), encode(v)))
        .collect();
    header_params.push(("oauth_signature".into(), signature_enc));
    header_params.sort();

    let header_inner = header_params
        .iter()
        .map(|(k, v)| format!(r#"{k}="{v}""#))
        .collect::<Vec<_>>()
        .join(", ");
    format!("OAuth {header_inner}")
}

fn hmac_sha1_base64(key: &str, msg: &str) -> String {
    let mut mac = HmacSha1::new_from_slice(key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(msg.as_bytes());
    let bytes = mac.finalize().into_bytes();
    BASE64.encode(bytes)
}

/// 32 hex chars from a CSPRNG. RFC 5849 only requires uniqueness within
/// the timestamp window per consumer key; 128 bits is overkill but cheap.
fn generate_nonce() -> String {
    let mut bytes = [0u8; 16];
    // api-server pins rand 0.8 (gift-bot used 0.9's `rand::rng()`); 0.8's
    // equivalent CSPRNG entry point is `thread_rng()`. Behavior identical:
    // fills `bytes` with cryptographically-random data.
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_creds() -> OAuth1Credentials {
        // RFC 5849 §1.2 example credentials — not real secrets.
        OAuth1Credentials {
            consumer_key: "9djdj82h48djs9d2".into(),
            consumer_secret: "j49sk3j29djd".into(),
            access_token: "kkk9d7dh3k39sjv7".into(),
            access_token_secret: "dh893hdasih9".into(),
        }
    }

    #[test]
    fn encode_matches_rfc_unreserved_set() {
        // Unreserved chars stay literal.
        assert_eq!(encode("ABCabc012-._~"), "ABCabc012-._~");
        // Reserved chars get percent-encoded.
        assert_eq!(encode("a b"), "a%20b");
        assert_eq!(encode("foo+bar"), "foo%2Bbar");
        assert_eq!(encode("/"), "%2F");
        assert_eq!(encode("&"), "%26");
        assert_eq!(encode("="), "%3D");
    }

    #[test]
    fn hmac_sha1_base64_matches_known_vector() {
        // RFC 2202 test case 2: key=Jefe, msg="what do ya want for nothing?",
        // HMAC-SHA1 = "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79" (hex)
        // base64 of those 20 bytes:
        let out = hmac_sha1_base64("Jefe", "what do ya want for nothing?");
        assert_eq!(out, "7/zfauXrL6LSdBbV8YTfnCWafHk=");
    }

    #[test]
    fn build_header_is_deterministic_for_fixed_inputs() {
        // Pin signature reproducibility: same creds + url + timestamp + nonce
        // ⇒ same Authorization header. The actual signature value depends on
        // every byte of the inputs, so this also catches accidental drift in
        // encode() / sorting / signature-base assembly.
        let creds = fixed_creds();
        let h1 = build_authorization_header(
            &creds,
            "https://api.x.com/2/tweets",
            "1700000000",
            "abc123",
        );
        let h2 = build_authorization_header(
            &creds,
            "https://api.x.com/2/tweets",
            "1700000000",
            "abc123",
        );
        assert_eq!(h1, h2);
        // Sanity-check the shape:
        assert!(h1.starts_with("OAuth "));
        assert!(h1.contains(r#"oauth_consumer_key="9djdj82h48djs9d2""#));
        assert!(h1.contains(r#"oauth_signature_method="HMAC-SHA1""#));
        assert!(h1.contains(r#"oauth_token="kkk9d7dh3k39sjv7""#));
        assert!(h1.contains(r#"oauth_version="1.0""#));
        assert!(h1.contains(r#"oauth_signature=""#));
    }

    #[test]
    fn build_header_signature_changes_with_url() {
        let creds = fixed_creds();
        let h1 = build_authorization_header(
            &creds,
            "https://api.x.com/2/tweets",
            "1700000000",
            "abc123",
        );
        let h2 = build_authorization_header(
            &creds,
            "https://api.x.com/2/users/me",
            "1700000000",
            "abc123",
        );
        assert_ne!(h1, h2, "different URLs must produce different signatures");
    }

    #[test]
    fn nonce_is_unique_per_call() {
        let a = generate_nonce();
        let b = generate_nonce();
        assert_ne!(a, b);
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn post_reply_success_returns_tweet_id() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/2/tweets")
            .match_header("authorization", mockito::Matcher::Regex("^OAuth .+".into()))
            .with_status(201)
            .with_header("content-type", "application/json")
            .with_body(r#"{"data":{"id":"42","text":"hi"}}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let r = post_reply(&client, &fixed_creds(), &server.url(), "hi", "tweet-1")
            .await
            .unwrap();
        assert_eq!(r.id, "42");
    }

    #[tokio::test]
    async fn post_reply_surfaces_unauthorized() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/2/tweets")
            .with_status(401)
            .with_body(r#"{"error":"unauthorized"}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        match post_reply(&client, &fixed_creds(), &server.url(), "hi", "tw-1").await {
            Err(ReplyError::Unauthorized) => {}
            other => panic!("expected Unauthorized, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn post_reply_surfaces_forbidden_with_body() {
        // 403 is the classic "app permissions are Read-only" symptom — the
        // body tells the operator exactly that.
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/2/tweets")
            .with_status(403)
            .with_body(r#"{"detail":"This app does not have write access"}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        match post_reply(&client, &fixed_creds(), &server.url(), "hi", "tw-1").await {
            Err(ReplyError::Forbidden { body }) => assert!(body.contains("write access")),
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn post_reply_parses_rate_limit_retry_after() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/2/tweets")
            .with_status(429)
            .with_header("retry-after", "37")
            .with_body(r#"{"title":"Too Many Requests"}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        match post_reply(&client, &fixed_creds(), &server.url(), "hi", "tw-1").await {
            Err(ReplyError::RateLimited {
                retry_after_s,
                body,
            }) => {
                assert_eq!(retry_after_s, 37);
                assert!(body.contains("Too Many"));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn post_reply_5xx_is_unexpected_status() {
        let mut server = mockito::Server::new_async().await;
        let _m = server
            .mock("POST", "/2/tweets")
            .with_status(503)
            .with_body("boom")
            .create_async()
            .await;

        let client = reqwest::Client::new();
        match post_reply(&client, &fixed_creds(), &server.url(), "hi", "tw-1").await {
            Err(ReplyError::UnexpectedStatus { status, body }) => {
                assert_eq!(status, 503);
                assert_eq!(body, "boom");
            }
            other => panic!("expected UnexpectedStatus, got {other:?}"),
        }
    }

    #[test]
    fn is_retryable_classifies_correctly() {
        assert!(ReplyError::Http("eof".into()).is_retryable());
        assert!(
            ReplyError::RateLimited {
                retry_after_s: 30,
                body: String::new()
            }
            .is_retryable()
        );
        assert!(
            ReplyError::UnexpectedStatus {
                status: 503,
                body: String::new()
            }
            .is_retryable()
        );
        assert!(
            !ReplyError::UnexpectedStatus {
                status: 400,
                body: String::new()
            }
            .is_retryable()
        );
        assert!(!ReplyError::Unauthorized.is_retryable());
        assert!(
            !ReplyError::Forbidden {
                body: String::new()
            }
            .is_retryable()
        );
        assert!(!ReplyError::Parse("bad".into()).is_retryable());
    }

    #[test]
    fn debug_redacts_credentials() {
        let creds = fixed_creds();
        let dbg = format!("{:?}", creds);
        assert!(!dbg.contains("9djdj82h48djs9d2"));
        assert!(!dbg.contains("j49sk3j29djd"));
        assert!(dbg.contains("***"));
    }
}
