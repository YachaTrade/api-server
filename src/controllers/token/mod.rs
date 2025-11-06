pub mod create;
pub mod metadata;
pub mod order;

use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::{AccountInfo, TokenInfo},
        token::TokenResponse,
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

#[derive(Debug, sqlx::FromRow)]
struct TokenRow {
    token_id: String,
    name: String,
    symbol: String,
    description: Option<String>,
    twitter: Option<String>,
    telegram: Option<String>,
    website: Option<String>,
    image_uri: String,
    is_graduated: bool,
    is_nsfw: bool,
    created_at: i64,
    creator: String,
    creator_nickname: String,
    creator_image_uri: String,
    creator_bio: String,
}

pub struct TokenController {
    db: Arc<PostgresDatabase>,
}

impl TokenController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenController { db }
    }

    pub async fn get_token(&self, token_id: &str) -> Result<TokenResponse> {
        let cache_key = cache_key!("token", token_id);

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_token(token_id).await
        })
        .await?;

        Ok(response)
    }

    async fn fetch_token(&self, token_id: &str) -> Result<TokenResponse> {
        let row = measure_postgres!(
            "token.fetch_token",
            sqlx::query_as::<_, TokenRow>(
                r#"
                SELECT
                    t.token_id,
                    t.name,
                    t.symbol,
                    t.description,
                    t.twitter,
                    t.telegram,
                    t.website,
                    t.image_uri,
                    t.is_graduated,
                    t.is_nsfw,
                    t.created_at,
                    t.creator,
                    COALESCE(
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                        a.nickname
                    ) as creator_nickname,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    a.bio as creator_bio,
                    a.follower_count as creator_follower_count,
                    a.following_count as creator_following_count
                FROM token t
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                WHERE t.token_id = $1
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get token: {}", err))?;

        let token_info = TokenInfo {
            token_id: row.token_id,
            name: row.name,
            symbol: row.symbol,
            image_uri: row.image_uri,
            description: row.description,
            is_graduated: row.is_graduated,
            is_nsfw: row.is_nsfw,
            twitter: row.twitter,
            telegram: row.telegram,
            website: row.website,
            created_at: row.created_at,
            creator: AccountInfo {
                account_id: row.creator,
                nickname: row.creator_nickname,
                bio: row.creator_bio,
                image_uri: row.creator_image_uri,
            },
        };

        Ok(TokenResponse { token_info })
    }
}
