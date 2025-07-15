use std::{str::FromStr, sync::Arc, time::Duration};

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{
        info::{AccountInfo, TokenInfo},
        pagination::PaginationParams,
    },
    utils::valid_evm_address,
};
use anyhow::{anyhow, Result};
use bigdecimal::BigDecimal;
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::FromRow;
use sqlx::Row;
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct PositionSwap {
    pub token: TokenInfo,
    pub account_id: String,
    pub is_buy: bool,
    pub native_amount: BigDecimal,
    pub token_amount: BigDecimal,
    pub created_at: i64,
    pub transaction_hash: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
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

/// Combined query parameters for swap history
#[derive(Debug, Clone, Deserialize, ToSchema, Default)]
pub struct SwapQuery {
    // PaginationParams fields
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit", deserialize_with = "validate_limit")]
    pub limit: i64,
    #[serde(default = "default_direction", deserialize_with = "validate_direction")]
    pub direction: String,
    /// Minimum volume filter (native amount)
    #[serde(default)]
    pub min_volume: Option<String>,

    /// Account ID for own trades filter
    #[serde(default)]
    pub account_id: Option<String>,

    /// Trade type filter: "BUY", "SELL", or "ALL" (default)
    #[serde(default = "default_trade_type")]
    pub trade_type: String,
}

impl SwapQuery {
    /// Validate the query parameters
    pub fn validate(&self) -> Result<(), String> {
        // Validate min_volume if provided
        if let Some(min_vol) = &self.min_volume {
            if min_vol.parse::<f64>().is_err() {
                return Err("Invalid min_volume: must be a valid number".to_string());
            }
        }

        if self.account_id.is_some() {
            if !valid_evm_address(self.account_id.as_ref().unwrap()) {
                return Err("Invalid account ID format".to_string());
            }
        }

        // Validate trade_type
        if !["BUY", "SELL", "ALL"].contains(&self.trade_type.as_str()) {
            return Err("Invalid trade_type: must be 'BUY', 'SELL', or 'ALL'".to_string());
        }

        Ok(())
    }
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    10
}

fn default_direction() -> String {
    "DESC".to_string()
}

fn default_trade_type() -> String {
    "ALL".to_string()
}

fn validate_limit<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let limit = i64::deserialize(deserializer)?;
    if limit < 1 || limit > 100 {
        return Err(serde::de::Error::custom(
            "Invalid limit: must be between 1 and 100",
        ));
    }
    Ok(limit)
}

fn validate_direction<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let direction = String::deserialize(deserializer)?;
    let direction_upper = direction.to_uppercase();
    if !["ASC", "DESC"].contains(&direction_upper.as_str()) {
        return Err(serde::de::Error::custom(
            "Invalid direction: must be 'ASC' or 'DESC'",
        ));
    }
    Ok(direction_upper)
}

pub struct SwapController {
    pub db: Arc<PostgresDatabase>,
}

impl SwapController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        SwapController { db }
    }

    pub async fn get_total_count_by_account(&self, account_id: &str) -> Result<i64> {
        let count = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query!(
                r#"
                SELECT COALESCE(COUNT(*)::bigint, 0) as count
                FROM swap s
                WHERE s.account_id = $1
                "#,
                account_id
            )
            .fetch_one(self.db.get_read_pool())
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;
        
        let count = count.count.unwrap_or(0);

        Ok(count)
    }
    pub async fn get_swaps_by_account(
        &self,
        account_id: &str,
        pagination: PaginationParams,
    ) -> Result<PositionSwapResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        let swaps = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query!(
                r#"
                SELECT 
                    s.account_id,
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
                WHERE s.account_id = $1
                ORDER BY s.created_at DESC
                LIMIT $2
                OFFSET $3
                "#,
                account_id,
                pagination.limit as i64,
                offset
            )
            .fetch_all(self.db.get_read_pool())
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

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
                account_id: row.account_id,
                is_buy: row.is_buy,
                native_amount: row.native_amount,
                token_amount: row.token_amount,
                created_at: row.created_at,
                transaction_hash: row.transaction_hash,
            })
            .collect();

        Ok(PositionSwapResponse { swaps, total_count })
    }

    pub async fn get_swaps_by_token(
        &self,
        token_id: &str,
        query: &SwapQuery,
    ) -> Result<TokenSwapResponse> {
        let offset = (query.page - 1) * query.limit;

        // 파라미터 카운터로 순서 관리
        let mut param_count = 1;
        let mut query_sql = r#"
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
            a.following_count,
            ax.x_handle,
            ax.x_image_uri,
            ax.is_blue_label
        FROM swap s
        JOIN account a ON s.account_id = a.account_id
        LEFT JOIN account_x ax ON a.account_id = ax.account_id
        WHERE s.token_id = $1"#
            .to_string();

        param_count += 1; // token_id는 $1

        // Add own trades filter
        if let Some(_) = &query.account_id {
            query_sql.push_str(&format!(" AND s.account_id = ${}", param_count));
            param_count += 1;
        }

        // Add trade type filter
        match query.trade_type.as_str() {
            "BUY" => query_sql.push_str(" AND s.is_buy = true"),
            "SELL" => query_sql.push_str(" AND s.is_buy = false"),
            _ => {} // "all" - no filter
        }
        // Add volume filters
        if let Some(_) = &query.min_volume {
            query_sql.push_str(&format!(" AND s.native_amount >= ${}", param_count));
            param_count += 1;
        }
        // 정렬 방향 추가
        query_sql.push_str(&format!(" ORDER BY s.created_at {}", query.direction));
        query_sql.push_str(&format!(" LIMIT {} OFFSET {}", query.limit, offset));

        // 쿼리 준비 및 파라미터 바인딩 (순서대로)
        let mut query_builder = sqlx::query(&query_sql);

        // 첫 번째 파라미터 바인딩 (token_id)
        query_builder = query_builder.bind(token_id);

        // 조건부 파라미터 바인딩 (순서 보장)
        if let Some(min_vol) = &query.min_volume {
            let min_vol_decimal = BigDecimal::from_str(min_vol)?;
            query_builder = query_builder.bind(min_vol_decimal);
        }

        if let Some(account_id) = &query.account_id {
            query_builder = query_builder.bind(account_id);
        }

        // 쿼리 실행
        let rows = tokio::time::timeout(
            Duration::from_millis(500),
            query_builder.fetch_all(self.db.get_read_pool())
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        // 결과 변환 (기존과 동일)
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
                let x_handle: Option<String> = row.try_get("x_handle").unwrap();
                let x_image_uri: Option<String> = row.try_get("x_image_uri").unwrap();
                let is_blue_label: Option<bool> = row.try_get("is_blue_label").unwrap();

                TokenSwap {
                    swap_id,
                    account_info: AccountInfo {
                        account_id,
                        nickname: if x_handle.is_some() {
                            x_handle.unwrap()
                        } else {
                            account_nickname
                        },
                        image_uri: if x_image_uri.is_some() {
                            x_image_uri.unwrap()
                        } else {
                            account_image
                        },
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
            self.get_total_count_by_token_with_filters(token_id, &query)
                .await?
        };

        Ok(TokenSwapResponse { swaps, total_count })
    }

    // get_total_count_by_token_with_filters도 동일하게 수정
    async fn get_total_count_by_token_with_filters(
        &self,
        token_id: &str,
        query_params: &SwapQuery,
    ) -> Result<i64> {
        // 필터가 없으면 캐시된 count 사용
        // 완전히 필터가 없는 경우 - 전체 count
        if query_params.min_volume.is_none()
            && query_params.account_id.is_none()
            && query_params.trade_type == "ALL"
        {
            return self.get_cached_count(token_id, "count").await;
        }

        // 거래 타입만 있는 경우 - buy_count 또는 sell_count
        if query_params.min_volume.is_none() && query_params.account_id.is_none() {
            let column = match query_params.trade_type.as_str() {
                "BUY" => "buy_count",
                "SELL" => "sell_count",
                _ => "count", // "ALL"
            };
            return self.get_cached_count(token_id, column).await;
        }

        let mut param_count = 1;
        let mut query = r#"
        SELECT COALESCE(COUNT(*)::bigint, 0) as count
        FROM swap s
        JOIN account a ON s.account_id = a.account_id
        WHERE s.token_id = $1"#
            .to_string();

        param_count += 1; // token_id는 $1

        // Add volume filters
        if let Some(_) = &query_params.min_volume {
            query.push_str(&format!(" AND s.native_amount >= ${}", param_count));
            param_count += 1;
        }

        // Add own trades filter
        if let Some(_) = &query_params.account_id {
            query.push_str(&format!(" AND s.account_id = ${}", param_count));
            param_count += 1;
        }

        // Add trade type filter
        match query_params.trade_type.as_str() {
            "BUY" => query.push_str(" AND s.is_buy = true"),
            "SELL" => query.push_str(" AND s.is_buy = false"),
            _ => {} // "all" - no filter
        }

        // Execute query using raw SQL
        let mut query_builder = sqlx::query(&query);

        // 첫 번째 파라미터 바인딩 (token_id)
        query_builder = query_builder.bind(token_id);

        // 조건부 파라미터 바인딩 (순서 보장)
        if let Some(min_vol) = &query_params.min_volume {
            let min_vol_decimal = BigDecimal::from_str(min_vol)?;
            query_builder = query_builder.bind(min_vol_decimal);
        }

        if let Some(account_id) = &query_params.account_id {
            query_builder = query_builder.bind(account_id);
        }

        let row = tokio::time::timeout(
            Duration::from_millis(500),
            query_builder.fetch_one(self.db.get_read_pool())
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;
        let count: i64 = row.try_get("count").unwrap();
        Ok(count)
    }

    async fn get_cached_count(&self, token_id: &str, column: &str) -> Result<i64> {
        let query = format!(
            "SELECT {} as count FROM swap_count WHERE token_id = $1",
            column
        );

        let row = tokio::time::timeout(
            Duration::from_millis(500),
            sqlx::query(&query)
                .bind(token_id)
                .fetch_optional(self.db.get_read_pool())
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 500ms"))??;

        Ok(row.map(|r| r.get::<i64, _>("count")).unwrap_or(0))
    }
}
