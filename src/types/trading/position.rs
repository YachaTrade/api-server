use std::sync::Arc;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::{
        CountRow,
        info::{AccountInfo, MarketInfo, PositionInfo, PositionTokenInfo, TokenInfo},
        pagination::PaginationParams,
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};
use anyhow::{Result, anyhow};

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
    pub current_amount: String,
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

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HoldToken {
    pub token_info: TokenInfo,
    pub balance: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct HoldTokenResponse {
    pub tokens: Vec<HoldToken>,
    pub total_count: i64,
}
pub struct PositionController {
    pub db: Arc<PostgresDatabase>,
}

impl PositionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        PositionController { db }
    }

    pub async fn get_total_count_by_token_holder(&self, token_id: &str) -> Result<i64> {
        // 캐시 키 생성
        let cache_key = cache_key!("token_holder_count", token_id);

        // Single Flight Pattern 적용
        let count = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_token_holder_count(token_id).await
        })
        .await?;

        Ok(count)
    }

    async fn fetch_token_holder_count(&self, token_id: &str) -> Result<i64> {
        // token_holder_count 테이블 사용으로 최적화
        let count = measure_postgres!(
            "position.fetch_token_holder_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COALESCE(holder_count, 0) as count
                FROM token_holder_count
                WHERE token_id = $1
                "#,
            )
            .bind(token_id)
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token holder count: {}", err))?;

        // 레코드가 없으면 0 반환
        Ok(count.map(|c| c.count).unwrap_or(0))
    }

    pub async fn get_holders_by_token(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenHolderResponse> {
        // 캐시 키 생성
        let cache_key = cache_key!("token_holders", token_id, pagination.page, pagination.limit);

        // Single Flight Pattern 적용
        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_holders_by_token(token_id, pagination).await
        })
        .await?;

        Ok(response)
    }

    async fn fetch_holders_by_token(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenHolderResponse> {
        let offset = (pagination.page - 1) * pagination.limit;
        let record = measure_postgres!(
            "position.fetch_holders_by_token",
            sqlx::query_as::<_, TokenHolderRow>(
                r#"
                SELECT 
                    b.balance as current_token_amount,
                    a.account_id,
                    a.nickname,
                    a.image_uri,
                    a.follower_count,
                    a.following_count,
                    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                    ax.x_image_uri,
                    ax.is_blue_label
                FROM balance b
                JOIN account a ON b.account_id = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                WHERE b.token_id = $1 AND b.balance > 0
                ORDER BY b.balance DESC
                OFFSET $2 LIMIT $3
                "#,
            )
            .bind(token_id)
            .bind(offset)
            .bind(pagination.limit as i64)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token holders: {}", err))?;
        let total_count = if record.is_empty() {
            0
        } else {
            self.get_total_count_by_token_holder(token_id).await?
        };

        #[derive(FromRow)]
        struct CreatorRow {
            creator: String,
        }

        let token_creator = measure_postgres!(
            "position.fetch_token_creator",
            sqlx::query_as::<_, CreatorRow>(
                r#"
                SELECT 
                    t.creator
                FROM token t
                WHERE t.token_id = $1   
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token creator: {}", err))?;

        let token_creator = token_creator.creator;

        let holders = record
            .into_iter()
            .map(|row| TokenHolder {
                current_amount: row.current_token_amount.to_plain_string(),
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

    pub async fn get_total_count_by_hold_token(&self, account_id: &str) -> Result<i64> {
        let count = measure_postgres!(
            "position.get_total_count_by_hold_token",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COALESCE(COUNT(*)::bigint, 0) as count
                FROM balance b
                WHERE b.account_id = $1 AND b.balance > 0
                "#,
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get hold token count: {}", err))?;

        let count = count.count;

        Ok(count)
    }
    pub async fn get_hold_token_by_account(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HoldTokenResponse> {
        let offset = (pagination.page - 1) * pagination.limit;
        #[derive(sqlx::FromRow)]
        pub struct HoldTokenRow {
            pub token_id: String,
            pub name: String,
            pub symbol: String,
            pub image_uri: String,
            pub balance: BigDecimal,
            pub price: BigDecimal,
        }

        let record = measure_postgres!(
            "position.get_hold_token_by_account",
            sqlx::query_as::<_, HoldTokenRow>(
                r#"
                SELECT 
                    t.token_id,
                    t.name,
                    t.symbol,
                    t.image_uri,
                    b.balance,
                    m.price
                FROM token t
                JOIN balance b ON t.token_id = b.token_id
                JOIN market m ON t.token_id = m.token_id
                WHERE b.account_id = $1 AND b.balance > 0
                ORDER BY (b.balance * m.price) DESC
                LIMIT $2 OFFSET $3
                "#,
            )
            .bind(account_id)
            .bind(pagination.limit)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get hold token by account: {}", err))?;

        let total_count = if record.is_empty() {
            0
        } else {
            self.get_total_count_by_hold_token(account_id).await?
        };

        let tokens = record
            .into_iter()
            .map(|row| HoldToken {
                token_info: TokenInfo {
                    token_id: row.token_id,
                    name: row.name,
                    symbol: row.symbol,
                    image_uri: row.image_uri,
                },
                balance: row.balance.to_string(),
                value: (row.balance * row.price).to_string(),
            })
            .collect();
        Ok(HoldTokenResponse {
            tokens,
            total_count,
        })
    }
}
