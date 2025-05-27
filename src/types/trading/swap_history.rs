use std::{str::FromStr, sync::Arc};

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{
        info::{AccountInfo, TokenInfo},
        pagination::PaginationParams,
    },
};
use anyhow::Result;
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::Row;
use utoipa::ToSchema;

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

/// Filter parameters for swap history
#[derive(Deserialize, ToSchema, Debug, Default)]
pub struct SwapFilterParams {
    /// Minimum volume filter (native amount)
    #[serde(default)]
    pub min_volume: Option<String>,

    /// Filter for own trades only (requires account_id)
    #[serde(default)]
    pub own_trades_only: bool,

    /// Account ID for own trades filter
    #[serde(default)]
    pub account_id: Option<String>,

    /// Trade type filter: "buy", "sell", or "all" (default)
    #[serde(default = "default_trade_type")]
    pub trade_type: String,
}

fn default_trade_type() -> String {
    "all".to_string()
}

impl SwapFilterParams {
    pub fn validate(&self) -> Result<()> {
        // Validate trade_type
        let valid_types = ["all", "buy", "sell"];
        if !valid_types.contains(&self.trade_type.as_str()) {
            return Err(anyhow::anyhow!(
                "Invalid trade_type. Must be 'all', 'buy', or 'sell'"
            ));
        }

        // Validate own_trades_only requires account_id
        if self.own_trades_only && self.account_id.is_none() {
            return Err(anyhow::anyhow!(
                "account_id is required when own_trades_only is true"
            ));
        }

        Ok(())
    }
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
        filters: &SwapFilterParams,
    ) -> Result<TokenSwapResponse> {
        // Validate filters
        filters.validate()?;

        let offset = (pagination.page - 1) * pagination.limit;

        // Build dynamic query with placeholders
        let mut query = r#"
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
        WHERE s.token_id = $1"#
            .to_string();

        // 파라미터 인덱스 추적
        let mut params = Vec::new();
        params.push(token_id);
        let mut param_index = 2; // $1은 이미 token_id에 사용됨

        // Add volume filters
        if let Some(min_vol) = &filters.min_volume {
            query.push_str(&format!(" AND s.native_amount >= ${}", param_index));
            params.push(min_vol);
            param_index += 1;
        }

        // Add own trades filter
        if filters.own_trades_only {
            if let Some(account_id) = &filters.account_id {
                query.push_str(&format!(" AND s.sender = ${}", param_index));
                params.push(account_id);
                param_index += 1;
            }
        }

        // Add trade type filter
        match filters.trade_type.as_str() {
            "buy" => query.push_str(" AND s.is_buy = true"),
            "sell" => query.push_str(" AND s.is_buy = false"),
            _ => {} // "all" - no filter
        }

        // 페이지네이션 추가 (고정된 인덱스 사용)
        query.push_str(" ORDER BY s.created_at DESC");
        query.push_str(&format!(
            " LIMIT ${} OFFSET ${}",
            param_index,
            param_index + 1
        ));

        // 쿼리 준비 및 파라미터 바인딩
        let mut query_builder = sqlx::query(&query);

        // 첫 번째 파라미터 바인딩 (token_id)
        query_builder = query_builder.bind(token_id);

        // 필터 파라미터 바인딩
        if let Some(min_vol) = &filters.min_volume {
            let min_vol = BigDecimal::from_str(&min_vol)?;
            query_builder = query_builder.bind(min_vol);
        }

        if filters.own_trades_only {
            if let Some(account_id) = &filters.account_id {
                query_builder = query_builder.bind(account_id);
            }
        }

        // 페이지네이션 파라미터 바인딩
        query_builder = query_builder.bind(pagination.limit);
        query_builder = query_builder.bind(offset);

        // 쿼리 실행
        let rows = query_builder.fetch_all(self.db.get_read_pool()).await?;

        // 결과 변환
        let swaps: Vec<TokenSwap> = rows
            .into_iter()
            .map(|row| {
                let swap_id: i32 = row.try_get("swap_id").unwrap();
                let account_id: String = row.try_get("account_id").unwrap();
                let account_nickname: String = row.try_get("account_nickname").unwrap();
                let account_image: String = row.try_get("account_image").unwrap();
                let follower_count: i32 = row.try_get("follower_count").unwrap();
                let following_count: i32 = row.try_get("following_count").unwrap();
                let is_buy: bool = row.try_get("is_buy").unwrap();
                let native_amount: BigDecimal = row.try_get("native_amount").unwrap();
                let token_amount: BigDecimal = row.try_get("token_amount").unwrap();
                let created_at: i64 = row.try_get("created_at").unwrap();
                let transaction_hash: String = row.try_get("transaction_hash").unwrap();

                TokenSwap {
                    swap_id,
                    account_info: AccountInfo {
                        account_id,
                        nickname: account_nickname,
                        image_uri: account_image,
                        follower_count,
                        following_count,
                    },
                    is_buy,
                    native_amount,
                    token_amount,
                    created_at,
                    transaction_hash,
                }
            })
            .collect();

        let total_count = if swaps.is_empty() {
            0
        } else {
            self.get_total_count_by_token_with_filters(token_id, filters)
                .await?
        };

        Ok(TokenSwapResponse { swaps, total_count })
    }

    async fn get_total_count_by_token_with_filters(
        &self,
        token_id: &str,
        filters: &SwapFilterParams,
    ) -> Result<i64> {
        // Build dynamic query
        let mut query = r#"
        SELECT COALESCE(COUNT(*)::bigint, 0) as count
        FROM swap s
        JOIN account a ON s.sender = a.account_id
        WHERE s.token_id = $1"#
            .to_string();

        let mut param_count = 1;
        let mut params: Vec<Box<dyn std::any::Any>> = vec![Box::new(token_id.to_string())];

        // Add volume filters
        if let Some(min_vol) = &filters.min_volume {
            param_count += 1;
            query.push_str(&format!(" AND s.native_amount >= ${}", param_count));
            params.push(Box::new(min_vol.clone()));
        }

        // Add own trades filter
        if filters.own_trades_only {
            if let Some(account_id) = &filters.account_id {
                param_count += 1;
                query.push_str(&format!(" AND s.sender = ${}", param_count));
                params.push(Box::new(account_id.clone()));
            }
        }

        // Add trade type filter
        match filters.trade_type.as_str() {
            "buy" => query.push_str(" AND s.is_buy = true"),
            "sell" => query.push_str(" AND s.is_buy = false"),
            _ => {} // "all" - no filter
        }

        // Execute query using raw SQL
        let mut sql_query = sqlx::query(&query);

        // Bind parameters
        sql_query = sql_query.bind(token_id);

        if let Some(min_vol) = &filters.min_volume {
            sql_query = sql_query.bind(min_vol);
        }

        if filters.own_trades_only {
            if let Some(account_id) = &filters.account_id {
                sql_query = sql_query.bind(account_id);
            }
        }

        let row = sql_query.fetch_one(self.db.get_read_pool()).await?;

        let count: i64 = row.try_get("count").unwrap();
        Ok(count)
    }
}
