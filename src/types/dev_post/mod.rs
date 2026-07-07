use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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

fn validate_images(image_uris: &Option<Vec<String>>) -> Result<(), String> {
    if let Some(imgs) = image_uris {
        if imgs.len() > MAX_IMAGES {
            return Err(format!("At most {MAX_IMAGES} images allowed"));
        }
        if imgs.iter().any(|u| u.trim().is_empty()) {
            return Err("Empty image_uri".into());
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
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenSummary {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: Option<String>,
    pub market_cap: Option<String>,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthorSummary {
    pub account_id: String,
    pub nickname: Option<String>,
    pub image_uri: Option<String>,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct PollOptionResponse {
    pub position: i16,
    pub label: String,
    pub image_uri: Option<String>,
    pub vote_count: i64,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct PollResponse {
    pub closes_at: chrono::DateTime<chrono::Utc>,
    pub is_closed: bool,
    pub total_votes: i64,
    pub options: Vec<PollOptionResponse>,
    pub my_vote_option: Option<i16>,
}
#[derive(Debug, Serialize, ToSchema)]
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
    pub posts: Vec<DevPostResponse>,
    pub total_count: i64,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct TrendingResponse {
    pub posts: Vec<DevPostResponse>,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct RankingRow {
    pub rank: i64,
    pub token: TokenSummary,
    pub total_likes: i64,
    pub post_count: i64,
    pub last_posted_at: Option<chrono::DateTime<chrono::Utc>>,
}
#[derive(Debug, Serialize, ToSchema)]
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
}
