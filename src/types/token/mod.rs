pub mod create_token;
pub mod hype;
pub mod metadata;
pub mod order;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

use super::common::info::AccountInfo;

// Structure for mapping SQL query results
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

#[derive(Debug, Clone, Deserialize, Serialize, sqlx::FromRow, ToSchema)]
pub struct TokenWithAccountInfo {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub website: Option<String>,
    pub is_listing: bool,
    pub created_at: i64,
    pub transaction_hash: String,
    pub account_info: AccountInfo,
    pub is_king: bool,
    pub is_king_created_at: Option<i64>,
    pub total_supply: BigDecimal,
    pub price: BigDecimal,
    pub market_cap: String,
}
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct TokenResponse {
    pub token: TokenWithAccountInfo,
}
pub struct TokenController {
    pub db: Arc<PostgresDatabase>,
}

impl TokenController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenController { db }
    }
    pub async fn get_token(&self, token_id: &str) -> Result<TokenResponse> {
        // Using query_as instead of query! to automatically map to the TokenRow struct
        let row = sqlx::query_as::<_, TokenRow>(
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
                    a.following_count as creator_following_count,
                    ax.x_handle,
                    ax.x_image_uri,
                FROM token t
                LEFT JOIN king k ON t.token_id = k.token_id
                LEFT JOIN account_x ax ON t.creator = ax.account_id
                JOIN market m ON t.token_id = m.token_id
                JOIN account a ON t.creator = a.account_id
                WHERE t.token_id = $1
            "#,
        )
        .bind(token_id)
        .fetch_one(self.db.get_read_pool())
        .await
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
            total_supply: row.total_supply,
            price: row.price,
        };
        let response = TokenResponse { token };
        Ok(response)
    }
}
