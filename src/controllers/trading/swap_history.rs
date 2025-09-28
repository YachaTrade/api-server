use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;
use sqlx::Row;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::{
        CountRow,
        info::{AccountInfo, TokenInfo},
        pagination::PaginationParams,
    },
    types::trading::swap_history::{
        PositionSwap, PositionSwapResponse, SwapQuery, TokenSwap, TokenSwapResponse,
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

pub struct SwapController {
    pub db: Arc<PostgresDatabase>,
}

impl SwapController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        SwapController { db }
    }

    pub async fn get_total_count_by_account(&self, account_id: &str) -> Result<i64> {
        let cache_key = cache_key!("swap_count_by_account", account_id);

        let count = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_total_count_by_account(account_id).await
        })
        .await?;

        Ok(count)
    }

    async fn fetch_total_count_by_account(&self, account_id: &str) -> Result<i64> {
        let count = measure_postgres!(
            "swap.fetch_total_count_by_account",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COALESCE(total_count, 0) as count
                FROM account_swap_count
                WHERE account_id = $1
                "#,
            )
            .bind(account_id)
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch total count by account: {}", err))?;

        Ok(count.map(|c| c.count).unwrap_or(0))
    }

    pub async fn get_swaps_by_account(
        &self,
        account_id: &str,
        pagination: PaginationParams,
    ) -> Result<PositionSwapResponse> {
        let cache_key = cache_key!(
            "swaps_by_account",
            account_id,
            pagination.page,
            pagination.limit
        );

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async {
            self.fetch_swaps_by_account(account_id, pagination).await
        })
        .await?;

        Ok(response)
    }

    async fn fetch_swaps_by_account(
        &self,
        account_id: &str,
        pagination: PaginationParams,
    ) -> Result<PositionSwapResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct SwapRow {
            account_id: String,
            token_id: String,
            token_symbol: String,
            token_image: String,
            token_name: String,
            is_buy: bool,
            native_amount: BigDecimal,
            token_amount: BigDecimal,
            created_at: i64,
            transaction_hash: String,
        }

        let swaps = measure_postgres!(
            "swap.fetch_swaps_by_account",
            sqlx::query_as::<_, SwapRow>(
                r#"
                WITH recent_swaps AS (
                    SELECT 
                        s.account_id,
                        s.token_id,
                        s.is_buy,
                        s.native_amount,
                        s.token_amount,
                        s.created_at,
                        s.transaction_hash
                    FROM swap s
                    WHERE s.account_id = $1
                    ORDER BY s.created_at DESC
                    LIMIT $2
                    OFFSET $3
                )
                SELECT 
                    rs.account_id,
                    rs.token_id,
                    t.symbol as token_symbol,
                    t.image_uri as token_image,
                    t.name as token_name,
                    rs.is_buy,
                    rs.native_amount,
                    rs.token_amount,
                    rs.created_at,
                    rs.transaction_hash
                FROM recent_swaps rs
                JOIN token t ON rs.token_id = t.token_id
                ORDER BY rs.created_at DESC
                "#,
            )
            .bind(account_id)
            .bind(pagination.limit as i64)
            .bind(offset)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch swaps by account: {}", err))?;

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
                native_amount: row.native_amount.to_plain_string(),
                token_amount: row.token_amount.to_plain_string(),
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

        let mut next_param = 2;
        let mut query_sql = r#"
        SELECT 
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
        LEFT JOIN LATERAL (
            SELECT 
                CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                x_image_uri, 
                is_blue_label 
            FROM account_x ax
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            WHERE ax.account_id = a.account_id 
            LIMIT 1
        ) ax ON true
        WHERE s.token_id = $1"#
            .to_string();

        if query.account_id.is_some() {
            query_sql.push_str(&format!(" AND s.account_id = ${}", next_param));
            next_param += 1;
        }

        match query.trade_type.as_str() {
            "BUY" => query_sql.push_str(" AND s.is_buy = true"),
            "SELL" => query_sql.push_str(" AND s.is_buy = false"),
            _ => {}
        }

        if query.min_volume.is_some() {
            query_sql.push_str(&format!(" AND s.native_amount >= ${}", next_param));
        }

        query_sql.push_str(&format!(" ORDER BY s.created_at {}", query.direction));
        query_sql.push_str(&format!(" LIMIT {} OFFSET {}", query.limit, offset));

        let mut query_builder = sqlx::query(&query_sql);
        query_builder = query_builder.bind(token_id);

        if let Some(account_id) = &query.account_id {
            query_builder = query_builder.bind(account_id);
        }

        if let Some(min_vol) = &query.min_volume {
            let min_vol_decimal = BigDecimal::from_str(min_vol)?;
            query_builder = query_builder.bind(min_vol_decimal);
        }

        let rows = measure_postgres!(
            "swap.get_swaps_by_token",
            query_builder.fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch swaps by token: {}", err))?;

        let swaps: Vec<TokenSwap> = rows
            .into_iter()
            .map(|row| {
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

                TokenSwap {
                    account_info: AccountInfo {
                        account_id,
                        nickname: x_handle
                            .clone()
                            .filter(|h| !h.is_empty())
                            .unwrap_or(account_nickname),
                        image_uri: x_image_uri
                            .clone()
                            .filter(|img| !img.is_empty())
                            .unwrap_or(account_image),
                        follower_count,
                        following_count,
                    },
                    is_buy,
                    native_amount: native_amount.to_plain_string(),
                    token_amount: token_amount.to_plain_string(),
                    created_at,
                    transaction_hash,
                }
            })
            .collect();

        let total_count = if swaps.is_empty() {
            0
        } else {
            self.get_total_count_by_token_with_filters(token_id, query)
                .await?
        };

        Ok(TokenSwapResponse { swaps, total_count })
    }

    async fn get_total_count_by_token_with_filters(
        &self,
        token_id: &str,
        query_params: &SwapQuery,
    ) -> Result<i64> {
        if query_params.min_volume.is_none()
            && query_params.account_id.is_none()
            && query_params.trade_type == "ALL"
        {
            return self.get_cached_count(token_id, "count").await;
        }

        if query_params.min_volume.is_none() && query_params.account_id.is_none() {
            let column = match query_params.trade_type.as_str() {
                "BUY" => "buy_count",
                "SELL" => "sell_count",
                _ => "count",
            };
            return self.get_cached_count(token_id, column).await;
        }

        let mut next_param = 2;
        let mut query = r#"
        SELECT COALESCE(COUNT(*)::bigint, 0) as count
        FROM swap s
        JOIN account a ON s.account_id = a.account_id
        WHERE s.token_id = $1"#
            .to_string();

        if query_params.account_id.is_some() {
            query.push_str(&format!(" AND s.account_id = ${}", next_param));
            next_param += 1;
        }

        if query_params.min_volume.is_some() {
            query.push_str(&format!(" AND s.native_amount >= ${}", next_param));
        }

        match query_params.trade_type.as_str() {
            "BUY" => query.push_str(" AND s.is_buy = true"),
            "SELL" => query.push_str(" AND s.is_buy = false"),
            _ => {}
        }

        let mut query_builder = sqlx::query(&query);
        query_builder = query_builder.bind(token_id);

        if let Some(account_id) = &query_params.account_id {
            query_builder = query_builder.bind(account_id);
        }

        if let Some(min_vol) = &query_params.min_volume {
            let min_vol_decimal = BigDecimal::from_str(min_vol)?;
            query_builder = query_builder.bind(min_vol_decimal);
        }

        let row = measure_postgres!(
            "swap.get_total_count_by_token_with_filters",
            query_builder.fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch swap count with filters: {}", err))?;
        let count: i64 = row.try_get("count").unwrap();
        Ok(count)
    }

    async fn get_cached_count(&self, token_id: &str, column: &str) -> Result<i64> {
        let query = format!(
            "SELECT {} as count FROM swap_count WHERE token_id = $1",
            column
        );

        let row = measure_postgres!(
            "swap.get_cached_count",
            sqlx::query(&query)
                .bind(token_id)
                .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch cached swap count: {}", err))?;

        Ok(row.map(|r| r.get::<i64, _>("count")).unwrap_or(0))
    }
}
