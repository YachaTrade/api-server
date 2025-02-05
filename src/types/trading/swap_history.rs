use std::sync::Arc;

use anyhow::Result;
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{info::TokenInfo, pagination::PaginationParams},
};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Swap {
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
pub struct SwapResponse {
    pub swaps: Vec<Swap>,
    pub total_count: i64,
}

pub struct SwapController {
    pub db: Arc<PostgresDatabase>,
}

impl SwapController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        SwapController { db }
    }

    pub async fn get_total_count(&self, account_id: &str) -> Result<i64> {
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
    pub async fn get_swaps(
        &self,
        account_id: &str,
        pagination: PaginationParams,
    ) -> Result<SwapResponse> {
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
            self.get_total_count(account_id).await?
        };

        let swaps = swaps
            .into_iter()
            .map(|row| Swap {
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

        Ok(SwapResponse { swaps, total_count })
    }
}
