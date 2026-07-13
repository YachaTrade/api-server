use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::db::r2::{DEVPOST_IMAGE_KEY_PREFIX, PUBLIC_BASE_URL};

pub const MAX_IMAGES: usize = 4;
pub const MIN_POLL_OPTIONS: usize = 2;
pub const MAX_POLL_OPTIONS: usize = 3;
pub const POLL_DURATION_DAYS: i64 = 14;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePollOptionRequest {
    pub label: String,
    pub image_uri: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePollRequest {
    pub options: Vec<CreatePollOptionRequest>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDevPostRequest {
    pub token_id: String,
    pub body: Option<String>,
    pub image_uris: Option<Vec<String>>,
    pub poll: Option<CreatePollRequest>,
}

/// Image URIs are echoed back to every reader and rendered by the frontend, so they
/// must come from our own upload endpoint rather than an arbitrary attacker-chosen
/// origin. `POST /dev-post/image` is the only way to mint one, and its keys are
/// always a fresh `Uuid::new_v4()` — so the accepted form is exactly
/// `{expected_prefix}<uuid>`, matching upload_devpost_image_file's output.
fn validate_image_uri(uri: &str) -> Result<(), String> {
    let expected_prefix = format!("{PUBLIC_BASE_URL}{DEVPOST_IMAGE_KEY_PREFIX}");
    let err = || format!("image_uri must be {expected_prefix}<uuid>");
    let key = uri.strip_prefix(&expected_prefix).ok_or_else(err)?;
    // Compared against the hyphenated form on purpose: `parse_str` also accepts the
    // braced, urn, and unhyphenated spellings, none of which the uploader ever mints.
    match uuid::Uuid::parse_str(key) {
        Ok(id) if id.hyphenated().to_string() == key => Ok(()),
        _ => Err(err()),
    }
}

fn validate_images(image_uris: &Option<Vec<String>>) -> Result<(), String> {
    if let Some(imgs) = image_uris {
        if imgs.len() > MAX_IMAGES {
            return Err(format!("At most {MAX_IMAGES} images allowed"));
        }
        for uri in imgs {
            validate_image_uri(uri)?;
        }
    }
    Ok(())
}

fn validate_poll(poll: &Option<CreatePollRequest>) -> Result<(), String> {
    if let Some(p) = poll {
        if p.options.len() < MIN_POLL_OPTIONS || p.options.len() > MAX_POLL_OPTIONS {
            return Err(format!(
                "Poll must have {MIN_POLL_OPTIONS}..={MAX_POLL_OPTIONS} options"
            ));
        }
        if p.options.iter().any(|o| o.label.trim().is_empty()) {
            return Err("Empty poll option label".into());
        }
        for uri in p.options.iter().filter_map(|o| o.image_uri.as_deref()) {
            validate_image_uri(uri)?;
        }
    }
    Ok(())
}

impl CreateDevPostRequest {
    pub fn validate(&self) -> Result<(), String> {
        let has_body = self
            .body
            .as_ref()
            .map(|b| !b.trim().is_empty())
            .unwrap_or(false);
        let has_images = self
            .image_uris
            .as_ref()
            .map(|v| !v.is_empty())
            .unwrap_or(false);
        let has_poll = self.poll.is_some();
        if !has_body && !has_images && !has_poll {
            return Err("Post must have a body, at least one image, or a poll".into());
        }
        validate_images(&self.image_uris)?;
        validate_poll(&self.poll)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EditDevPostRequest {
    pub body: Option<String>,
    pub image_uris: Option<Vec<String>>,
}
impl EditDevPostRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.body.is_none() && self.image_uris.is_none() {
            return Err("Nothing to update".into());
        }
        validate_images(&self.image_uris)
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VoteRequest {
    pub option_position: i16,
}

// ---- responses ----
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TokenSummary {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: Option<String>,
    pub market_cap: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AuthorSummary {
    pub account_id: String,
    pub nickname: Option<String>,
    pub image_uri: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PollOptionResponse {
    pub position: i16,
    pub label: String,
    pub image_uri: Option<String>,
    pub vote_count: i64,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PollResponse {
    pub closes_at: chrono::DateTime<chrono::Utc>,
    pub is_closed: bool,
    pub total_votes: i64,
    pub options: Vec<PollOptionResponse>,
    pub my_vote_option: Option<i16>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct DevPostResponse {
    pub id: String, // BIGINT as string
    pub token: TokenSummary,
    pub author: AuthorSummary,
    pub body: String,
    pub tweet_url: Option<String>,
    pub images: Vec<String>,
    pub poll: Option<PollResponse>,
    pub like_count: i64,
    pub liked_by_me: bool,
    pub is_edited: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct DevPostListResponse {
    #[schema(required, nullable)]
    pub pin: Option<DevPostResponse>,
    pub posts: Vec<DevPostResponse>,
    pub total_count: i64,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FeedBase {
    #[schema(required, nullable)]
    pub pin: Option<DevPostResponse>,
    pub posts: Vec<DevPostResponse>,
    pub total_count: i64,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct TrendingResponse {
    pub posts: Vec<DevPostResponse>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RankingRow {
    pub rank: i64,
    pub token: TokenSummary,
    pub total_likes: i64,
    pub post_count: i64,
    pub last_posted_at: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct RankingResponse {
    pub rankings: Vec<RankingRow>,
    pub total_count: i64,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct LikeResponse {
    pub like_count: i64,
    pub liked_by_me: bool,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct VoteResponse {
    pub total_votes: i64,
    pub options: Vec<PollOptionResponse>,
    pub my_vote_option: Option<i16>,
}
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = dev_post::DevPostUploadImageResponse)]
pub struct UploadImageResponse {
    pub image_uri: String,
}

/// Extract the first x.com/twitter.com URL from a post body (no network fetch).
pub fn parse_tweet_url(body: &str) -> Option<String> {
    body.split_whitespace()
        .find(|tok| {
            let t = tok.trim_end_matches(['.', ',', ')', ']', '"', '\'']);
            t.starts_with("https://x.com/")
                || t.starts_with("https://twitter.com/")
                || t.starts_with("https://www.x.com/")
                || t.starts_with("https://www.twitter.com/")
        })
        .map(|t| {
            t.trim_end_matches(['.', ',', ')', ']', '"', '\''])
                .to_string()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_response(id: &str) -> DevPostResponse {
        let at = chrono::DateTime::parse_from_rfc3339("2026-07-13T00:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        DevPostResponse {
            id: id.to_string(),
            token: TokenSummary {
                token_id: "0xToken".into(),
                name: "Token".into(),
                symbol: "TKN".into(),
                image_uri: Some("img".into()),
                market_cap: None,
            },
            author: AuthorSummary {
                account_id: "0xCreator".into(),
                nickname: None,
                image_uri: None,
            },
            body: "announcement".into(),
            tweet_url: None,
            images: Vec::new(),
            poll: None,
            like_count: 0,
            liked_by_me: false,
            is_edited: false,
            created_at: at,
            updated_at: at,
        }
    }

    fn opt(label: &str) -> CreatePollOptionRequest {
        CreatePollOptionRequest {
            label: label.into(),
            image_uri: None,
        }
    }

    #[test]
    fn rejects_completely_empty_post() {
        let r = CreateDevPostRequest {
            token_id: "0x0".into(),
            body: None,
            image_uris: None,
            poll: None,
        };
        assert!(r.validate().is_err());
    }
    #[test]
    fn accepts_body_only() {
        let r = CreateDevPostRequest {
            token_id: "0x0".into(),
            body: Some("gm".into()),
            image_uris: None,
            poll: None,
        };
        assert!(r.validate().is_ok());
    }
    #[test]
    fn rejects_too_many_images() {
        let imgs = vec!["u".to_string(); MAX_IMAGES + 1];
        let r = CreateDevPostRequest {
            token_id: "0x0".into(),
            body: Some("x".into()),
            image_uris: Some(imgs),
            poll: None,
        };
        assert!(r.validate().is_err());
    }
    #[test]
    fn rejects_poll_with_one_option() {
        let r = CreateDevPostRequest {
            token_id: "0x0".into(),
            body: Some("x".into()),
            image_uris: None,
            poll: Some(CreatePollRequest {
                options: vec![opt("A")],
            }),
        };
        assert!(r.validate().is_err());
    }
    #[test]
    fn rejects_poll_with_four_options() {
        let r = CreateDevPostRequest {
            token_id: "0x0".into(),
            body: Some("x".into()),
            image_uris: None,
            poll: Some(CreatePollRequest {
                options: vec![opt("A"), opt("B"), opt("C"), opt("D")],
            }),
        };
        assert!(r.validate().is_err());
    }
    #[test]
    fn rejects_poll_with_empty_label() {
        let r = CreateDevPostRequest {
            token_id: "0x0".into(),
            body: Some("x".into()),
            image_uris: None,
            poll: Some(CreatePollRequest {
                options: vec![opt("A"), opt("")],
            }),
        };
        assert!(r.validate().is_err());
    }
    #[test]
    fn accepts_poll_with_two_options() {
        let r = CreateDevPostRequest {
            token_id: "0x0".into(),
            body: None,
            image_uris: None,
            poll: Some(CreatePollRequest {
                options: vec![opt("A"), opt("B")],
            }),
        };
        assert!(r.validate().is_ok());
    }
    fn with_images(uris: Vec<&str>) -> CreateDevPostRequest {
        CreateDevPostRequest {
            token_id: "0x0".into(),
            body: Some("x".into()),
            image_uris: Some(uris.into_iter().map(String::from).collect()),
            poll: None,
        }
    }

    #[test]
    fn accepts_image_uri_from_devpost_storage_path() {
        let r = with_images(vec![
            "https://storage.nadapp.net/devpost/550e8400-e29b-41d4-a716-446655440000",
        ]);
        assert!(r.validate().is_ok());
    }

    /// `POST /dev-post/image` must be the only source of image URIs — otherwise the
    /// upload endpoint (and its format validation) can be bypassed entirely.
    #[test]
    fn rejects_image_uri_from_a_foreign_origin() {
        for uri in [
            "https://evil.com/x.svg",
            "javascript:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "http://storage.nadapp.net/devpost/abc", // not https
            "https://storage.nadapp.net.evil.com/x",
            "",
        ] {
            assert!(
                with_images(vec![uri]).validate().is_err(),
                "should have rejected image_uri {uri:?}"
            );
        }
    }

    #[test]
    fn rejects_image_uri_from_other_paths_on_our_storage_domain() {
        for uri in [
            "https://storage.nadapp.net/account/abc",
            "https://storage.nadapp.net/coin/abc",
            "https://storage.nadapp.net/metadata/abc.json",
            "https://storage.nadapp.net/devpost-evil/abc",
            "https://storage.nadapp.net/devpost",
            "https://storage.nadapp.net/devpost/../account/abc",
            "https://storage.nadapp.net/devpost/%2e%2e/account/abc",
        ] {
            assert!(
                with_images(vec![uri]).validate().is_err(),
                "should have rejected image_uri {uri:?}"
            );
        }
    }

    #[test]
    fn rejects_noncanonical_devpost_image_uri_suffixes() {
        for uri in [
            "https://storage.nadapp.net/devpost/abc?download=1",
            "https://storage.nadapp.net/devpost/abc#fragment",
        ] {
            assert!(
                with_images(vec![uri]).validate().is_err(),
                "should have rejected image_uri {uri:?}"
            );
        }
    }

    #[test]
    fn rejects_image_uri_with_injected_chars_in_key() {
        let r = with_images(vec![
            "https://storage.nadapp.net/devpost/abc\"><script>alert(1)</script>",
        ]);
        assert!(r.validate().is_err());
    }

    #[test]
    fn rejects_image_uri_with_empty_key() {
        let r = with_images(vec!["https://storage.nadapp.net/devpost/"]);
        assert!(r.validate().is_err());
    }

    /// `Uuid::parse_str` accepts these, but `upload_devpost_image_file` never emits
    /// them — the accepted set must be exactly what the uploader mints.
    #[test]
    fn rejects_noncanonical_uuid_spellings() {
        for key in [
            "{550e8400-e29b-41d4-a716-446655440000}",
            "urn:uuid:550e8400-e29b-41d4-a716-446655440000",
            "550e8400e29b41d4a716446655440000",
            "550E8400-E29B-41D4-A716-446655440000",
        ] {
            let uri = format!("https://storage.nadapp.net/devpost/{key}");
            assert!(
                with_images(vec![&uri]).validate().is_err(),
                "should have rejected image_uri {uri:?}"
            );
        }
    }

    #[test]
    fn rejects_poll_option_image_uri_from_a_foreign_origin() {
        let r = CreateDevPostRequest {
            token_id: "0x0".into(),
            body: Some("x".into()),
            image_uris: None,
            poll: Some(CreatePollRequest {
                options: vec![
                    opt("A"),
                    CreatePollOptionRequest {
                        label: "B".into(),
                        image_uri: Some("https://evil.com/b.svg".into()),
                    },
                ],
            }),
        };
        assert!(r.validate().is_err());
    }

    #[test]
    fn edit_rejects_image_uri_from_a_foreign_origin() {
        let r = EditDevPostRequest {
            body: None,
            image_uris: Some(vec!["https://evil.com/x.png".into()]),
        };
        assert!(r.validate().is_err());
    }

    #[test]
    fn edit_rejects_too_many_images() {
        let r = EditDevPostRequest {
            body: None,
            image_uris: Some(vec!["u".to_string(); MAX_IMAGES + 1]),
        };
        assert!(r.validate().is_err());
    }
    #[test]
    fn parses_tweet_url() {
        assert_eq!(
            parse_tweet_url("check https://x.com/a/status/1 !"),
            Some("https://x.com/a/status/1".into())
        );
        assert_eq!(parse_tweet_url("no link"), None);
    }

    #[test]
    fn devpost_response_serde_round_trips() {
        let r = DevPostResponse {
            id: "1".into(),
            token: TokenSummary {
                token_id: "0xT".into(),
                name: "n".into(),
                symbol: "s".into(),
                image_uri: None,
                market_cap: None,
            },
            author: AuthorSummary {
                account_id: "0xA".into(),
                nickname: None,
                image_uri: None,
            },
            body: "b".into(),
            tweet_url: None,
            images: vec![],
            poll: None,
            like_count: 3,
            liked_by_me: true,
            is_edited: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let j = serde_json::to_string(&r).unwrap();
        let back: DevPostResponse = serde_json::from_str(&j).unwrap();
        assert_eq!(back.id, "1");
        assert_eq!(back.like_count, 3);
    }

    #[test]
    fn feed_base_round_trips() {
        let b = FeedBase {
            pin: None,
            posts: vec![],
            total_count: 7,
        };
        let back: FeedBase = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
        assert_eq!(back.total_count, 7);
    }

    #[test]
    fn dev_post_list_serializes_required_nullable_pin() {
        let without_pin = DevPostListResponse {
            pin: None,
            posts: Vec::new(),
            total_count: 0,
        };
        let json = serde_json::to_value(without_pin).unwrap();
        assert!(json.get("pin").is_some());
        assert!(json["pin"].is_null());

        let with_pin = DevPostListResponse {
            pin: Some(sample_response("7")),
            posts: vec![sample_response("6")],
            total_count: 1,
        };
        let json = serde_json::to_value(with_pin).unwrap();
        assert_eq!(json["pin"]["id"], "7");
        assert_eq!(json["posts"][0]["id"], "6");
    }

    #[test]
    fn feed_base_round_trips_pin_posts_and_count() {
        let base = FeedBase {
            pin: Some(sample_response("7")),
            posts: vec![sample_response("6")],
            total_count: 1,
        };
        let json = serde_json::to_string(&base).unwrap();
        let decoded: FeedBase = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.pin.unwrap().id, "7");
        assert_eq!(decoded.posts[0].id, "6");
        assert_eq!(decoded.total_count, 1);
    }
}
