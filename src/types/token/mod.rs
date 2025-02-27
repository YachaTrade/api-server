pub mod create_token;
pub mod order;

use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::db::postgres::PostgresDatabase;

use super::common::info::AccountInfo;

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
    pub create_transaction_hash: String,
    pub account_info: AccountInfo,
    pub is_king: bool,
    pub is_king_created_at: Option<i64>,
    pub total_supply: BigDecimal,
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
        let record = sqlx::query!(
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
                    t.created_at,
                    t.create_transaction_hash,
                    COALESCE(k.token_id IS NOT NULL, false)::boolean as is_king,
                    k.created_at as "is_king_created_at?",
                    t.creator as creator_account_id,
                    a.nickname as creator_nickname,
                    a.image_uri as creator_image_uri,
                    a.follower_count as creator_follower_count, 
                    a.following_count as creator_following_count
                    
                FROM token t
                LEFT JOIN king k ON t.token_id = k.token_id
                JOIN account a ON t.creator = a.account_id
                WHERE t.token_id = $1
            "#,
            token_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?;

        let token = TokenWithAccountInfo {
            token_id: record.token_id,
            name: record.name,
            symbol: record.symbol,
            image_uri: record.image_uri,
            description: record.description,
            twitter: record.twitter,
            telegram: record.telegram,
            website: record.website,
            is_listing: record.is_listing,
            created_at: record.created_at,
            create_transaction_hash: record.create_transaction_hash,
            account_info: AccountInfo {
                account_id: record.creator_account_id,
                nickname: record.creator_nickname,
                image_uri: record.creator_image_uri,
                follower_count: record.creator_follower_count,
                following_count: record.creator_following_count,
            },
            is_king: record.is_king.unwrap_or(false),
            is_king_created_at: record.is_king_created_at,
            total_supply: record.total_supply,
        };
        let response = TokenResponse { token };
        Ok(response)
    }
}
