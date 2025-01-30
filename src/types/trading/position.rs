use std::sync::Arc;

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{info::TokenInfo, pagination::PaginationParams},
};
use anyhow::Result;
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// Position information for a token held by a profile
#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Position {
    /// Token information
    pub token: TokenInfo,
    /// Current token price
    pub token_price: BigDecimal,
    /// Unique position identifier
    pub position_id: i64,
    /// Total amount spent in native currency for buying
    pub total_bought_native: BigDecimal,
    /// Total amount of tokens bought
    pub total_bought_token: BigDecimal,
    /// Current token amount held
    pub current_token_amount: BigDecimal,
    /// Current value in native currency (price * current_token_amount)
    pub current_value: BigDecimal,
    /// Realized profit/loss
    pub realized_pnl: BigDecimal,
    /// Unrealized profit/loss
    pub unrealized_pnl: BigDecimal,
    /// Total profit/loss (realized + unrealized)
    pub total_pnl: BigDecimal,
    /// Position creation timestamp
    pub created_at: i64,
    /// Last trade timestamp
    pub last_traded_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PositionResponse {
    pub positions: Vec<Position>,
    pub total_count: i64,
}

pub struct PositionController {
    pub db: Arc<PostgresDatabase>,
}

impl PositionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        PositionController { db }
    }
    pub async fn get_total_count(&self, account_id: &str) -> Result<i64> {
        let count = sqlx::query!(
            r#"
            SELECT COALESCE(COUNT(*)::bigint, 0) as count
            FROM position p
            WHERE p.account_id = $1
            AND p.current_token_amount > 0
            "#,
            account_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?
        .count
        .unwrap_or(0);

        Ok(count)
    }
    pub async fn get_positions(
        &self,
        account_id: &str,
        pagination: PaginationParams,
    ) -> Result<PositionResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        let positions = sqlx::query!(
            r#"
            WITH position_data AS (
                SELECT 
                    p.position_id,
                    p.token_id,
                    p.total_bought_native,
                    p.total_bought_token,
                    p.current_token_amount,
                    p.realized_pnl,
                    p.created_at,
                    p.last_traded_at,
                    t.symbol as token_symbol,
                    t.image_uri as token_image,
                    COALESCE(m.price, 0) as token_price,
                    
                    -- Calculate unrealized_pnl
                    COALESCE(
                        CASE 
                            WHEN p.current_token_amount = 0 THEN 0
                            ELSE (COALESCE(m.price, 0) * p.current_token_amount) - 
                                (p.total_bought_native * p.current_token_amount / p.total_bought_token)
                        END
                    , 0) as unrealized_pnl
                FROM position p
                JOIN token t ON p.token_id = t.token_id
                LEFT JOIN market m ON p.token_id = m.token_id
                WHERE p.account_id = $1
                AND p.current_token_amount > 0
            )
            SELECT 
                position_id,
                token_id,
                token_symbol,
                token_price as "token_price!",
                token_image,
                total_bought_native,
                total_bought_token,
                current_token_amount,
                COALESCE((token_price * current_token_amount), 0) as "current_value!",
                realized_pnl,
                unrealized_pnl as "unrealized_pnl!",
                (realized_pnl + unrealized_pnl) as "total_pnl!",
                created_at,
                last_traded_at
            FROM position_data
            ORDER BY (realized_pnl + unrealized_pnl) DESC
            LIMIT $2
            OFFSET $3
            "#,
            account_id,
            pagination.limit as i64,
            offset
        )
        .fetch_all(self.db.get_read_pool())
        .await?;

        let total_count = if positions.is_empty() {
            0
        } else {
            self.get_total_count(account_id).await?
        };

        let positions = positions
            .into_iter()
            .map(|row| Position {
                token: TokenInfo {
                    token_id: row.token_id,
                    symbol: row.token_symbol,
                    image_uri: row.token_image,
                },
                token_price: row.token_price,
                position_id: row.position_id,
                total_bought_native: row.total_bought_native,
                total_bought_token: row.total_bought_token,
                current_token_amount: row.current_token_amount,
                current_value: row.current_value,
                realized_pnl: row.realized_pnl,
                unrealized_pnl: row.unrealized_pnl,
                total_pnl: row.total_pnl,
                created_at: row.created_at,
                last_traded_at: row.last_traded_at,
            })
            .collect();

        Ok(PositionResponse {
            positions,
            total_count,
        })
    }
}
