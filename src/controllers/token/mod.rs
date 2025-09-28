pub mod create;
pub mod metadata;
pub mod order;

use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::AccountInfo,
        token::{TokenResponse, TokenWithAccountInfo},
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
    is_listing: bool,
    total_supply: BigDecimal,
    price: BigDecimal,
    created_at: i64,
    transaction_hash: String,
    is_king: Option<bool>,
    is_king_created_at: Option<i64>,
    creator: String,
    creator_nickname: String,
    creator_image_uri: String,
    creator_follower_count: i32,
    creator_following_count: i32,
    x_handle: Option<String>,
    x_image_uri: Option<String>,
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
                    WITH token_info AS (
                        SELECT 
                            t.token_id,
                            t.name,
                            t.symbol,
                            t.description,
                            t.twitter,
                            t.telegram,
                            t.website,
                            t.image_uri,
                            t.is_listing,
                            t.total_supply,
                            m.price,
                            t.created_at,
                            t.transaction_hash,
                            COALESCE(k.token_id IS NOT NULL, false)::boolean as is_king,
                            k.created_at as is_king_created_at,
                            t.creator,
                            a.nickname as creator_nickname,
                            a.image_uri as creator_image_uri,
                            a.follower_count as creator_follower_count, 
                            a.following_count as creator_following_count
                        FROM token t
                        LEFT JOIN king k ON t.token_id = k.token_id
                        JOIN market m ON t.token_id = m.token_id
                        JOIN account a ON t.creator = a.account_id
                        WHERE t.token_id = $1
                    )
                    SELECT 
                        ti.token_id,
                        ti.name,
                        ti.symbol,
                        ti.description,
                        ti.twitter,
                        ti.telegram,
                        ti.website,
                        ti.image_uri,
                        ti.is_listing,
                        ti.total_supply,
                        ti.price,
                        ti.created_at,
                        ti.transaction_hash,
                        ti.is_king,
                        ti.is_king_created_at,
                        ti.creator,
                        ti.creator_nickname,
                        ti.creator_image_uri,
                        ti.creator_follower_count,
                        ti.creator_following_count,
                        ax.x_handle,
                        ax.x_image_uri
                    FROM token_info ti
                    LEFT JOIN LATERAL (
                        SELECT 
                            CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                            ax.x_image_uri 
                        FROM account_x ax
                        LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                        WHERE ax.account_id = ti.creator 
                        LIMIT 1
                    ) ax ON true
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get token: {}", err))?;

        let token = TokenWithAccountInfo {
            token_id: row.token_id,
            name: row.name,
            symbol: row.symbol,
            image_uri: row.image_uri,
            description: row.description,
            twitter: row.twitter,
            telegram: row.telegram,
            website: row.website,
            is_listing: row.is_listing,
            created_at: row.created_at,
            transaction_hash: row.transaction_hash,
            account_info: AccountInfo {
                account_id: row.creator,
                nickname: match &row.x_handle {
                    Some(handle) if !handle.is_empty() => handle.clone(),
                    _ => row.creator_nickname,
                },
                image_uri: match &row.x_image_uri {
                    Some(img) if !img.is_empty() => img.clone(),
                    _ => row.creator_image_uri,
                },
                follower_count: row.creator_follower_count,
                following_count: row.creator_following_count,
            },
            is_king: row.is_king.unwrap_or(false),
            is_king_created_at: row.is_king_created_at,
            market_cap: (row.total_supply.clone() * row.price.clone()).to_string(),
            total_supply: row.total_supply.to_plain_string(),
            price: row.price.to_plain_string(),
        };

        Ok(TokenResponse { token })
    }
}
