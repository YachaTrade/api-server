use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::env;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Account {
    // #[serde(rename = "id")]
    pub account_id: String,

    // #[serde(rename = "imageUri")]
    pub image_uri: String,

    // #[serde(rename = "nickname")]
    pub nickname: String,

    // #[serde(rename = "bio")]
    pub bio: String,

    // #[serde(rename = "followerCount")]
    pub follower_count: i32,

    // #[serde(rename = "followingCount")]
    pub following_count: i32,

    // #[serde(rename = "likeCount")]
    pub like_count: i32,
}

impl Account {
    pub fn new(account_id: String) -> Self {
        Self {
            account_id: account_id.clone(),
            image_uri: env::get_env("DEFAULT_IMAGE"),
            nickname: account_id,
            bio: "".to_string(),
            follower_count: 0,
            following_count: 0,
            like_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct Token {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub creator: String,
    pub description: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub website: Option<String>,
    pub image_uri: String,
    pub is_listing: bool,
    pub pair: Option<String>,
    pub created_at: i64,
    pub create_transaction_hash: String,
    pub is_updated: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct Thread {
    #[serde(rename = "threadId")]
    pub thread_id: i32,
    #[serde(rename = "tokenId")]
    pub token_id: String,
    #[serde(rename = "authorId")]
    pub author_id: String,
    #[serde(rename = "content")]
    pub content: String,
    #[serde(rename = "createdAt")]
    #[schema(value_type = String, example = "2023-06-01T12:00:00Z")]
    pub created_at: DateTime<Utc>,
    #[serde(rename = "updatedAt")]
    #[schema(value_type = String, example = "2023-06-01T12:00:00Z")]
    pub updated_at: DateTime<Utc>,
    #[serde(rename = "rootId")]
    pub root_id: Option<i32>,
    #[serde(rename = "likesCount")]
    pub likes_count: i32,
    #[serde(rename = "replyCount")]
    pub reply_count: i32,
    #[serde(rename = "imageUri")]
    pub image_uri: Option<String>,
}
impl Thread {
    pub fn new(token_id: String, author_id: String, content: String, root_id: Option<i32>) -> Self {
        Self {
            thread_id: 0,
            token_id,
            author_id,
            content,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            root_id,
            likes_count: 0,
            reply_count: 0,
            image_uri: None,
        }
    }
}
