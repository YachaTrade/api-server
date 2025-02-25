use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{
        info::{AccountInfo, TokenInfo},
        pagination::PaginationParams,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct PositionSwap {
    pub token: TokenInfo,
    pub swap_id: i32,
    pub account_id: String,
    pub is_buy: bool,
    pub native_amount: BigDecimal,
    pub token_amount: BigDecimal,
    pub created_at: i64,
    pub transaction_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PositionSwapResponse {
    pub swaps: Vec<PositionSwap>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct TokenSwap {
    pub swap_id: i32,
    pub account_info: AccountInfo,
    pub is_buy: bool,
    pub native_amount: BigDecimal,
    pub token_amount: BigDecimal,
    pub created_at: i64,
    pub transaction_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenSwapResponse {
    pub swaps: Vec<TokenSwap>,
    pub total_count: i64,
}

pub struct SwapController {
    pub db: Arc<PostgresDatabase>,
}

impl SwapController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        SwapController { db }
    }

    pub async fn get_total_count_by_account(&self, account_id: &str) -> Result<i64> {
        let count = sqlx::query!(
            r#"
            SELECT COALESCE(COUNT(*)::bigint, 0) as count
            FROM swap s
            WHERE s.sender = $1
            "#,
            account_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?
        .count
        .unwrap_or(0);

        Ok(count)
    }
    pub async fn get_swaps_by_account(
        &self,
        account_id: &str,
        pagination: PaginationParams,
    ) -> Result<PositionSwapResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        let swaps = sqlx::query!(
            r#"
            SELECT 
                s.swap_id,
                s.sender,
                s.token_id,
                t.symbol as token_symbol,
                t.image_uri as token_image,
                t.name as token_name,
                s.is_buy,
                s.native_amount,
                s.token_amount,
                s.created_at,
                s.transaction_hash
            FROM swap s
            JOIN token t ON s.token_id = t.token_id
            WHERE s.sender = $1
            ORDER BY s.created_at DESC
            LIMIT $2
            OFFSET $3
            "#,
            account_id,
            pagination.limit as i64,
            offset
        )
        .fetch_all(self.db.get_read_pool())
        .await?;

        let total_count = if swaps.is_empty() {
            0
        } else {
            self.get_total_count_by_account(account_id).await?
        };

        let swaps = swaps
            .into_iter()
            .map(|row| PositionSwap {
                token: TokenInfo {
                    token_id: row.token_id,
                    symbol: row.token_symbol,
                    name: row.token_name,
                    image_uri: row.token_image,
                },
                swap_id: row.swap_id,
                account_id: row.sender,
                is_buy: row.is_buy,
                native_amount: row.native_amount,
                token_amount: row.token_amount,
                created_at: row.created_at,
                transaction_hash: row.transaction_hash,
            })
            .collect();

        Ok(PositionSwapResponse { swaps, total_count })
    }

    pub async fn get_total_count_by_token(&self, token_id: &str) -> Result<i64> {
        let count = sqlx::query!(
            r#"
            SELECT COALESCE(COUNT(*)::bigint, 0) as count
            FROM swap s
            WHERE s.token_id = $1
            "#,
            token_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?
        .count
        .unwrap_or(0);
        Ok(count)
    }
    pub async fn get_swaps_by_token(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenSwapResponse> {
        let offset = (pagination.page - 1) * pagination.limit;
        let swaps = sqlx::query!(
            r#"
            SELECT 
                s.swap_id,
                s.token_id,
                s.is_buy,
                s.native_amount,
                s.token_amount,
                s.created_at,
                s.transaction_hash,
                a.account_id,
                a.nickname as account_nickname,
                a.image_uri as account_image,
                a.follower_count,
                a.following_count
            FROM swap s
            JOIN account a ON s.sender = a.account_id
            WHERE s.token_id = $1
            ORDER BY s.created_at DESC
            LIMIT $2
            OFFSET $3
            "#,
            token_id,
            pagination.limit as i64,
            offset
        )
        .fetch_all(self.db.get_read_pool())
        .await?;
        let total_count = if swaps.is_empty() {
            0
        } else {
            self.get_total_count_by_token(token_id).await?
        };

        let swaps = swaps
            .into_iter()
            .map(|row| TokenSwap {
                swap_id: row.swap_id,
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: row.account_nickname,
                    image_uri: row.account_image,
                    follower_count: row.follower_count,
                    following_count: row.following_count,
                },
                is_buy: row.is_buy,
                native_amount: row.native_amount,
                token_amount: row.token_amount,
                created_at: row.created_at,
                transaction_hash: row.transaction_hash,
            })
            .collect();

        Ok(TokenSwapResponse { swaps, total_count })
    }
}
