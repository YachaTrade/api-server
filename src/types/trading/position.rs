use std::sync::Arc;

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{
        info::{AccountInfo, MarketInfo, PositionInfo, PositionTokenInfo},
        pagination::PaginationParams,
    },
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
    pub token: PositionTokenInfo,

    /// Unique position identifier
    pub position: PositionInfo,

    pub market: MarketInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct PositionResponse {
    pub positions: Vec<Position>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenHolder {
    pub current_amount: BigDecimal,
    pub account_info: AccountInfo,
    pub is_dev: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenHolderResponse {
    pub holders: Vec<TokenHolder>,
    pub total_count: i64,
}

pub struct PositionController {
    pub db: Arc<PostgresDatabase>,
}

impl PositionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        PositionController { db }
    }
    pub async fn get_total_count_by_account(&self, account_id: &str) -> Result<i64> {
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

        let record = sqlx::query!(
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
                    t.name as token_name,
                    t.image_uri as token_image,
                    t.created_at as token_created_at,
                    t.total_supply as token_total_supply,
                    m.market_id as token_market_id,
                    m.market_type as token_market_type,
                    m.virtual_token as token_virtual_token,
                    m.virtual_native as token_virtual_native,
                    m.reserve_token as token_reserve_token,
                    m.reserve_native as token_reserve_native,
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
                JOIN market m ON p.token_id = m.token_id
                WHERE p.account_id = $1 AND
                p.is_active = true
            )
            SELECT 
                position_id,
                token_id,
                token_symbol,
                token_price as "token_price!",
                token_image, 
                token_name,
                total_bought_native,
                total_bought_token,
                current_token_amount,
                COALESCE((token_price * current_token_amount), 0) as "current_value!",
                realized_pnl,
                unrealized_pnl as "unrealized_pnl!",
                (realized_pnl + unrealized_pnl) as "total_pnl!",
                created_at,
                last_traded_at,
                token_created_at,
                token_total_supply,
                token_market_id,
                token_market_type,
                token_virtual_token as "token_virtual_token!",
                token_virtual_native as "token_virtual_native!",
                token_reserve_token as "token_reserve_token!",
                token_reserve_native as "token_reserve_native!"
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

        let total_count = if record.is_empty() {
            0
        } else {
            self.get_total_count_by_account(account_id).await?
        };

        let positions = record
            .into_iter()
            .map(|row| Position {
                token: PositionTokenInfo {
                    token_id: row.token_id,
                    symbol: row.token_symbol,
                    name: row.token_name,
                    image_uri: row.token_image,
                    created_at: row.token_created_at,
                    total_supply: row.token_total_supply,
                },
                position: PositionInfo {
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
                },
                market: MarketInfo {
                    market_id: row.token_market_id,
                    market_type: row.token_market_type,
                    virtual_token: row.token_virtual_token,
                    virtual_native: row.token_virtual_native,
                    reserve_token: row.token_reserve_token,
                    reserve_native: row.token_reserve_native,
                    price: row.token_price,
                },
            })
            .collect();

        Ok(PositionResponse {
            positions,
            total_count,
        })
    }

    pub async fn get_total_count_by_token_holder(&self, token_id: &str) -> Result<i64> {
        let count = sqlx::query!(
            r#"
            SELECT COALESCE(COUNT(*)::bigint, 0) as count
            FROM position p
            WHERE p.token_id = $1 AND p.current_token_amount > 0
            AND p.current_token_amount > 0
            "#,
            token_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?
        .count
        .unwrap_or(0);

        Ok(count)
    }

    pub async fn get_holders_by_token(
        &self,
        token_id: &str,
        pagination: PaginationParams,
    ) -> Result<TokenHolderResponse> {
        let offset = (pagination.page - 1) * pagination.limit;
        let record = sqlx::query!(
            r#"
            SELECT 
                p.current_token_amount,
                a.account_id,
                a.nickname,
                a.image_uri,
                a.follower_count,
                a.following_count
            FROM position p
            JOIN account a ON p.account_id = a.account_id
            WHERE p.token_id = $1 AND p.current_token_amount > 0 AND p.is_active = true
            OFFSET $2 LIMIT $3
            "#,
            token_id,
            offset,
            pagination.limit as i64
        )
        .fetch_all(self.db.get_read_pool())
        .await?;
        let total_count = if record.is_empty() {
            0
        } else {
            self.get_total_count_by_token_holder(token_id).await?
        };

        let token_creator = sqlx::query!(
            r#"
            SELECT 
                t.creator as "creator!"
            FROM token t
            WHERE t.token_id = $1   
            "#,
            token_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?
        .creator;

        let holders = record
            .into_iter()
            .map(|row| TokenHolder {
                current_amount: row.current_token_amount,
                is_dev: row.account_id == token_creator,
                account_info: AccountInfo {
                    account_id: row.account_id,
                    nickname: row.nickname,
                    image_uri: row.image_uri,
                    follower_count: row.follower_count,
                    following_count: row.following_count,
                },
            })
            .collect();
        Ok(TokenHolderResponse {
            holders,
            total_count,
        })
    }
}
