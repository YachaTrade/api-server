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

#[derive(sqlx::FromRow)]
struct TokenHolderRow {
    current_token_amount: BigDecimal, // 타입에 맞게 수정 필요
    account_id: String,
    nickname: String,
    image_uri: String,
    follower_count: i32,
    following_count: i32,
    x_handle: Option<String>,
    x_image_uri: Option<String>,
    is_blue_label: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenHolderResponse {
    pub holders: Vec<TokenHolder>,
    pub total_count: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")] // 또는 "UPPERCASE"
pub enum PositionType {
    All,
    Open,
    Close,
}
impl Default for PositionType {
    fn default() -> Self {
        Self::All
    }
}

impl ToString for PositionType {
    fn to_string(&self) -> String {
        match self {
            Self::All => "ALL".to_string(),
            Self::Open => "OPEN".to_string(),
            Self::Close => "CLOSE".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PositionQuery {
    pub position_type: PositionType,
    #[serde(default = "default_limit", deserialize_with = "validate_limit")]
    pub limit: i64,
    #[serde(default = "default_page")]
    pub page: i64,
}
fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    10
}

fn validate_limit<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let limit = i64::deserialize(deserializer)?;
    if limit > 100 {
        Err(serde::de::Error::custom("Limit must be 100 or less"))
    } else {
        Ok(limit)
    }
}

pub struct PositionController {
    pub db: Arc<PostgresDatabase>,
}

impl PositionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        PositionController { db }
    }

    pub async fn get_positions(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        position_type: &PositionType,
    ) -> Result<PositionResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        // 데이터 조회와 카운트를 병렬로 수행
        let (record, total_count) = tokio::join!(
            // 기존 데이터 조회 쿼리
            sqlx::query!(
                r#"
                WITH position_data AS (
                    SELECT 
                        p.token_id,
                        p.total_bought_native,
                        p.total_bought_token,
                        (p.total_sold_native - ((p.total_bought_native / p.total_bought_token) * p.total_sold_token)) as realized_pnl,
                        p.created_at,
                        p.last_traded_at,
                        b.balance as current_token_amount,
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
                        -- Calculate unrealized_pnl using AMM 공식
                        COALESCE(
                            CASE 
                                WHEN b.balance = 0 THEN 0
                                WHEN m.market_type = 'CURVE' THEN
                                    m.virtual_native 
                                    - (
                                        ((m.virtual_token * m.virtual_native) 
                                        + (m.virtual_token + b.balance) - 1)
                                        / (m.virtual_token + b.balance)
                                    )
                                WHEN m.market_type = 'DEX' THEN
                                    m.price * b.balance
                                ELSE 0
                            END,
                        0) AS unrealized_pnl
                    FROM balance b
                    JOIN token t ON b.token_id = t.token_id
                    JOIN market m ON b.token_id = m.token_id
                    JOIN positions p ON b.token_id = p.token_id AND b.account_id = p.account_id
                    WHERE b.account_id = $1 
                      AND ($4::text = 'ALL' 
                           OR ($4::text = 'OPEN' AND b.balance > 0)
                           OR ($4::text = 'CLOSE' AND b.balance = 0))
                )
                SELECT 
                    token_id,
                    token_symbol,
                    token_price as "token_price!",
                    token_image, 
                    token_name,
                    total_bought_native,
                    total_bought_token,
                    current_token_amount,
                    -- current_value 계산식: 여기서는 unrealized_pnl와 동일하게 계산하도록 함
                    unrealized_pnl as "current_value!",
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
                offset,
                position_type.to_string()
            )
            .fetch_all(self.db.get_read_pool()),
            // 조건에 맞는 총 레코드 수를 조회하는 쿼리
            sqlx::query!(
                r#"
                SELECT COUNT(*) as "count!"
                FROM balance b
                WHERE b.account_id = $1
                  AND ($2::text = 'ALL' 
                       OR ($2::text = 'OPEN' AND b.balance > 0)
                       OR ($2::text = 'CLOSE' AND b.balance = 0))
                "#,
                account_id,
                position_type.to_string()
            )
            .fetch_one(self.db.get_read_pool())
        );
        let record = record?;
        let total_count = total_count?.count;
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
                    total_bought_native: row.total_bought_native,
                    total_bought_token: row.total_bought_token,
                    current_token_amount: row.current_token_amount,
                    current_value: row.unrealized_pnl.clone(),
                    realized_pnl: row.realized_pnl.unwrap_or(BigDecimal::from(0)),
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
            FROM balance b
            WHERE b.token_id = $1 AND b.balance > 0
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
        pagination: &PaginationParams,
    ) -> Result<TokenHolderResponse> {
        let offset = (pagination.page - 1) * pagination.limit;
        let record = sqlx::query_as::<_, TokenHolderRow>(
            r#"
            SELECT 
                b.balance as current_token_amount,
                a.account_id,
                a.nickname,
                a.image_uri,
                a.follower_count,
                a.following_count,
                ax.x_handle,
                ax.x_image_uri,
                ax.is_blue_label
            FROM balance b
            JOIN account a ON b.account_id = a.account_id
            LEFT JOIN account_x ax ON a.account_id = ax.account_id
            WHERE b.token_id = $1 AND b.balance > 0
            ORDER BY b.balance DESC
            OFFSET $2 LIMIT $3
            "#,
        )
        .bind(token_id)
        .bind(offset)
        .bind(pagination.limit as i64)
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
                    nickname: match &row.x_handle {
                        Some(handle) if !handle.is_empty() => handle.clone(),
                        _ => row.nickname,
                    },
                    image_uri: match &row.x_image_uri {
                        Some(img) if !img.is_empty() => img.clone(),
                        _ => row.image_uri,
                    },
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
