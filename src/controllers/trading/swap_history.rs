use std::str::FromStr;
use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::{
        CountRow,
        info::{AccountInfo, SwapInfo, SwapType, TokenInfo, TokenSwapInfo},
        pagination::PaginationParams,
    },
    types::{
        profile::SwapHistoryResponse,
        trading::swap_history::{SwapQuery, TokenSwap, TokenSwapResponse},
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
    ) -> Result<SwapHistoryResponse> {
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
    ) -> Result<SwapHistoryResponse> {
        let offset = (pagination.page - 1) * pagination.limit;

        #[derive(sqlx::FromRow)]
        struct SwapRow {
            token_id: String,
            token_name: String,
            token_symbol: String,
            token_image_uri: String,
            token_description: Option<String>,
            token_twitter: Option<String>,
            token_telegram: Option<String>,
            token_website: Option<String>,
            is_listing: bool,
            token_created_at: i64,
            creator: String,
            holder_count: i64,
            creator_nickname: String,
            creator_bio: String,
            creator_image_uri: String,
            creator_follower_count: i32,
            creator_following_count: i32,
            is_buy: bool,
            native_amount: BigDecimal,
            token_amount: BigDecimal,
            native_price: BigDecimal,
            created_at: i64,
            transaction_hash: String,
        }

        let swaps = measure_postgres!(
            "swap.fetch_swaps_by_account",
            sqlx::query_as::<_, SwapRow>(
                r#"
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                ),
                recent_swaps AS (
                    SELECT
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
                    t.token_id,
                    t.name as token_name,
                    t.symbol as token_symbol,
                    t.image_uri as token_image_uri,
                    t.description as token_description,
                    t.twitter as token_twitter,
                    t.telegram as token_telegram,
                    t.website as token_website,
                    t.is_listing,
                    t.created_at as token_created_at,
                    t.creator,
                    t.token_holder_count as holder_count,
                    COALESCE(
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                        a.nickname
                    ) as creator_nickname,
                    a.bio as creator_bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    a.follower_count as creator_follower_count,
                    a.following_count as creator_following_count,
                    rs.is_buy,
                    rs.native_amount,
                    rs.token_amount,
                    COALESCE(lp.price, 0) as native_price,
                    rs.created_at,
                    rs.transaction_hash
                FROM recent_swaps rs
                JOIN token t ON rs.token_id = t.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                CROSS JOIN latest_price lp
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
            .map(|row| TokenSwapInfo {
                token_info: TokenInfo {
                    token_id: row.token_id,
                    name: row.token_name,
                    symbol: row.token_symbol,
                    image_uri: row.token_image_uri,
                    description: row.token_description,
                    is_listing: row.is_listing,
                    twitter: row.token_twitter,
                    telegram: row.token_telegram,
                    website: row.token_website,
                    created_at: row.token_created_at,
                    holder_count: row.holder_count,
                    creator: AccountInfo {
                        account_id: row.creator,
                        nickname: row.creator_nickname,
                        bio: row.creator_bio,
                        image_uri: row.creator_image_uri,
                        follower_count: row.creator_follower_count,
                        following_count: row.creator_following_count,
                    },
                },
                swap_info: SwapInfo {
                    event_type: if row.is_buy {
                        SwapType::Buy
                    } else {
                        SwapType::Sell
                    },
                    native_amount: row.native_amount.to_string(),
                    token_amount: row.token_amount.to_string(),
                    native_price: row.native_price.to_string(),
                    transaction_hash: row.transaction_hash,
                    created_at: row.created_at,
                },
            })
            .collect();

        Ok(SwapHistoryResponse { swaps, total_count })
    }

    pub async fn get_swaps_by_token(
        &self,
        token_id: &str,
        query: &SwapQuery,
    ) -> Result<TokenSwapResponse> {
        let offset = (query.page - 1) * query.limit;

        #[derive(sqlx::FromRow)]
        struct TokenSwapRow {
            account_id: String,
            account_nickname: String,
            bio: String,
            account_image: String,
            follower_count: i32,
            following_count: i32,
            is_buy: bool,
            native_amount: BigDecimal,
            token_amount: BigDecimal,
            native_price: BigDecimal,
            created_at: i64,
            transaction_hash: String,
            x_handle: Option<String>,
            x_image_uri: Option<String>,
        }

        let mut next_param = 2;
        let mut query_sql = r#"
        WITH latest_price AS (
            SELECT price
            FROM price
            ORDER BY created_at DESC
            LIMIT 1
        )
        SELECT
            a.account_id,
            a.nickname as account_nickname,
            a.bio,
            a.image_uri as account_image,
            a.follower_count,
            a.following_count,
            s.is_buy,
            s.native_amount,
            s.token_amount,
            s.created_at,
            s.transaction_hash,
            ax.x_handle,
            ax.x_image_uri,
            COALESCE(lp.price, 0) as native_price
        FROM swap s
        JOIN account a ON s.account_id = a.account_id
        LEFT JOIN LATERAL (
            SELECT
                CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                x_image_uri
            FROM account_x ax
            LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
            WHERE ax.account_id = a.account_id
            LIMIT 1
        ) ax ON true
        CROSS JOIN latest_price lp
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

        let mut query_builder = sqlx::query_as::<_, TokenSwapRow>(&query_sql);
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
                TokenSwap {
                    account_info: AccountInfo {
                        account_id: row.account_id,
                        nickname: row.x_handle
                            .clone()
                            .filter(|h| !h.is_empty())
                            .unwrap_or(row.account_nickname),
                        bio: row.bio,
                        image_uri: row.x_image_uri
                            .clone()
                            .filter(|img| !img.is_empty())
                            .unwrap_or(row.account_image),
                        follower_count: row.follower_count,
                        following_count: row.following_count,
                    },
                    swap_info: SwapInfo {
                        event_type: if row.is_buy { SwapType::Buy } else { SwapType::Sell },
                        native_amount: row.native_amount.to_plain_string(),
                        token_amount: row.token_amount.to_plain_string(),
                        native_price: row.native_price.to_plain_string(),
                        transaction_hash: row.transaction_hash,
                        created_at: row.created_at,
                    },
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

        let mut query_builder = sqlx::query_as::<_, CountRow>(&query);
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

        Ok(row.count)
    }

    async fn get_cached_count(&self, token_id: &str, column: &str) -> Result<i64> {
        let query = format!(
            "SELECT {} as count FROM swap_count WHERE token_id = $1",
            column
        );

        let row = measure_postgres!(
            "swap.get_cached_count",
            sqlx::query_as::<_, CountRow>(&query)
                .bind(token_id)
                .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch cached swap count: {}", err))?;

        Ok(row.map(|r| r.count).unwrap_or(0))
    }
}
