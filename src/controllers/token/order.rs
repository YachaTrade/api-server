use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    config::BONDING_CURVE,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::{
            CountRow,
            info::{AccountInfo, MarketInfo, MarketType, TokenInfo},
            pagination::PaginationParams,
        },
        token::order::{OrderToken, OrderTokenResponse, TokenOrderType},
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

#[derive(Debug, sqlx::FromRow)]
struct OrderTokenRow {
    token_id: String,
    name: String,
    symbol: String,
    token_image_uri: String,
    description: Option<String>,
    twitter: Option<String>,
    telegram: Option<String>,
    website: Option<String>,
    is_listing: bool,
    created_at: i64,
    creator: String,
    holder_count: i64,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    creator_follower_count: i32,
    creator_following_count: i32,
    market_type: String,
    market_id: String,
    token_price: BigDecimal,
    native_price: BigDecimal,
    price: BigDecimal,
    total_supply: BigDecimal,
    liquidity: BigDecimal,
    volume: BigDecimal,
    price_24h_ago: Option<BigDecimal>,
}

pub struct OrderController {
    db: Arc<PostgresDatabase>,
}

impl OrderController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        OrderController { db }
    }

    pub async fn get_order_tokens(
        &self,
        order_by: TokenOrderType,
        pagination: &PaginationParams,
    ) -> Result<Vec<OrderToken>> {
        let cache_key = cache_key!(
            "order_tokens",
            order_by.as_str(),
            pagination.direction,
            pagination.page,
            pagination.limit
        );

        let order_by_clone = order_by;
        let tokens = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async move {
            self.fetch_order_tokens(order_by_clone, pagination).await
        })
        .await?;

        Ok(tokens)
    }

    async fn fetch_order_tokens(
        &self,
        order_by: TokenOrderType,
        pagination: &PaginationParams,
    ) -> Result<Vec<OrderToken>> {
        let offset = (pagination.page.abs() - 1) * pagination.limit;
        let order_direction = &pagination.direction;

        let rows = match order_by {
            TokenOrderType::CreationTime => {
                let current_time = current_unix_timestamp();
                let time_24h_ago = current_time - 86400;

                let query = format!(
                    r#"
                    WITH latest_price AS (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    )
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_listing,
                        t.created_at,
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
                        m.market_type,
                        COALESCE(m.pool_id, '') as market_id,
                        (m.price * COALESCE(lp.price, 0)) as token_price,
                        COALESCE(lp.price, 0) as native_price,
                        m.price,
                        t.total_supply,
                        COALESCE(m.reserve_native, 0) as liquidity,
                        m.volume,
                        (
                            SELECT ph.price
                            FROM price_history ph
                            WHERE ph.token_id = t.token_id
                            AND ph.created_at <= $3
                            ORDER BY
                                ph.created_at DESC,
                                ph.tx_index DESC NULLS LAST,
                                ph.log_index DESC
                            LIMIT 1
                        ) as price_24h_ago
                    FROM token t
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    JOIN market m ON t.token_id = m.token_id
                    CROSS JOIN latest_price lp
                    ORDER BY t.created_at {}
                    LIMIT $1 OFFSET $2
                    "#,
                    order_direction
                );

                measure_postgres!(
                    "token_order.fetch_creation_time",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit as i64)
                        .bind(offset)
                        .bind(time_24h_ago)
                        .fetch_all(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by creation time: {}", err))?
            }
            TokenOrderType::LatestTrade => {
                let current_time = current_unix_timestamp();
                let time_24h_ago = current_time - 86400;

                let query = format!(
                    r#"
                    WITH latest_price AS (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    )
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_listing,
                        t.created_at,
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
                        m.market_type,
                        COALESCE(m.pool_id, '') as market_id,
                        (m.price * COALESCE(lp.price, 0)) as token_price,
                        COALESCE(lp.price, 0) as native_price,
                        m.price,
                        t.total_supply,
                        COALESCE(m.reserve_native, 0) as liquidity,
                        m.volume,
                        (
                            SELECT ph.price
                            FROM price_history ph
                            WHERE ph.token_id = t.token_id
                            AND ph.created_at <= $3
                            ORDER BY
                                ph.created_at DESC,
                                ph.tx_index DESC NULLS LAST,
                                ph.log_index DESC
                            LIMIT 1
                        ) as price_24h_ago
                    FROM market m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    CROSS JOIN latest_price lp
                    ORDER BY m.latest_trade_at {}
                    LIMIT $1 OFFSET $2
                    "#,
                    order_direction
                );

                measure_postgres!(
                    "token_order.fetch_latest_trade",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit)
                        .bind(offset)
                        .bind(time_24h_ago)
                        .fetch_all(&*self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by latest trade: {}", err))?
            }
            TokenOrderType::MarketCap => {
                let current_time = current_unix_timestamp();
                let time_24h_ago = current_time - 86400;

                let query = format!(
                    r#"
                    WITH latest_price AS (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    )
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_listing,
                        t.created_at,
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
                        m.market_type,
                        COALESCE(m.pool_id, '') as market_id,
                        (m.price * COALESCE(lp.price, 0)) as token_price,
                        COALESCE(lp.price, 0) as native_price,
                        m.price,
                        t.total_supply,
                        COALESCE(m.reserve_native, 0) as liquidity,
                        m.volume,
                        (
                            SELECT ph.price
                            FROM price_history ph
                            WHERE ph.token_id = t.token_id
                            AND ph.created_at <= $3
                            ORDER BY
                                ph.created_at DESC,
                                ph.tx_index DESC NULLS LAST,
                                ph.log_index DESC
                            LIMIT 1
                        ) as price_24h_ago
                    FROM (
                        SELECT token_id, price, market_type, pool_id
                        FROM market
                        ORDER BY price {}
                        LIMIT $1 OFFSET $2
                    ) m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    CROSS JOIN latest_price lp
                    ORDER BY m.price {}
                    "#,
                    order_direction, order_direction
                );

                measure_postgres!(
                    "token_order.fetch_market_cap",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit)
                        .bind(offset)
                        .bind(time_24h_ago)
                        .fetch_all(&*self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by market cap: {}", err))?
            }
            TokenOrderType::Verified => {
                // Optimized with MATERIALIZED CTEs to force correct execution order
                // This prevents PostgreSQL from scanning the entire market table
                let current_time = current_unix_timestamp();
                let time_24h_ago = current_time - 86400;

                let query = format!(
                    r#"
                    WITH latest_price AS (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    ),
                    verified_creators AS MATERIALIZED (
                        SELECT a.account_id
                        FROM account_verified av
                        JOIN account_x ax ON av.x_handle = ax.x_handle
                        JOIN account a ON ax.account_id = a.account_id
                    ),
                    top_verified_markets AS MATERIALIZED (
                        SELECT m.token_id, m.price, m.market_type, m.pool_id, m.reserve_native, m.volume
                        FROM market m
                        JOIN token t ON m.token_id = t.token_id
                        WHERE t.creator IN (SELECT account_id FROM verified_creators)
                        ORDER BY m.price {}
                        LIMIT $1 OFFSET $2
                    )
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_listing,
                        t.created_at,
                        t.creator,
                        t.token_holder_count as holder_count,
                        REPLACE(ax.x_handle, '@', '#') as creator_nickname,
                        a.bio as creator_bio,
                        ax.x_image_uri as creator_image_uri,
                        a.follower_count as creator_follower_count,
                        a.following_count as creator_following_count,
                        tvm.market_type,
                        COALESCE(tvm.pool_id, '') as market_id,
                        (tvm.price * COALESCE(lp.price, 0)) as token_price,
                        COALESCE(lp.price, 0) as native_price,
                        tvm.price,
                        t.total_supply,
                        COALESCE(tvm.reserve_native, 0) as liquidity,
                        tvm.volume,
                        (
                            SELECT ph.price
                            FROM price_history ph
                            WHERE ph.token_id = t.token_id
                            AND ph.created_at <= $3
                            ORDER BY
                                ph.created_at DESC,
                                ph.tx_index DESC NULLS LAST,
                                ph.log_index DESC
                            LIMIT 1
                        ) as price_24h_ago
                    FROM top_verified_markets tvm
                    JOIN token t ON tvm.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    JOIN account_x ax ON a.account_id = ax.account_id
                    CROSS JOIN latest_price lp
                    ORDER BY tvm.price {}
                    "#,
                    order_direction, order_direction
                );

                measure_postgres!(
                    "token_order.fetch_verified",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit)
                        .bind(offset)
                        .bind(time_24h_ago)
                        .fetch_all(&*self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch verified tokens: {}", err))?
            }
        };

        Ok(rows.into_iter().map(OrderToken::from).collect())
    }

    pub async fn get_total_count_by_type(&self, order_type: &TokenOrderType) -> Result<i64> {
        let (query, log_type) = match order_type {
            TokenOrderType::Verified => (
                "SELECT verified_token_count as count FROM token_count",
                "verified_token_count",
            ),
            _ => (
                "SELECT total_count as count FROM token_count",
                "total_count",
            ),
        };

        let row = measure_postgres!(
            "token_order.get_total_count_by_type",
            sqlx::query_as::<_, CountRow>(query).fetch_one(self.db.get_read_pool())
        )
        .map_err(|e| anyhow!("Failed to get {} count: {}", log_type, e))?;

        Ok(row.count)
    }

    pub fn build_order_response(tokens: Vec<OrderToken>, total_count: i64) -> OrderTokenResponse {
        OrderTokenResponse {
            tokens,
            total_count,
        }
    }
}

impl From<OrderTokenRow> for OrderToken {
    fn from(row: OrderTokenRow) -> Self {
        let mut market_id = row.market_id.clone();
        if row.market_type == "CURVE" && market_id.is_empty() {
            market_id = BONDING_CURVE.clone();
        }

        let percent = match &row.price_24h_ago {
            Some(price_24h_ago) => {
                calculate_price_change_percent(
                    &price_24h_ago.to_plain_string(),
                    &row.price.to_plain_string(),
                )
                .unwrap_or(0.0)
            }
            None => 0.0,
        };

        OrderToken {
            account_info: AccountInfo {
                account_id: row.creator.clone(),
                nickname: row.creator_nickname.clone(),
                bio: row.creator_bio.clone(),
                image_uri: row.creator_image_uri.clone(),
                follower_count: row.creator_follower_count,
                following_count: row.creator_following_count,
            },
            token_info: TokenInfo {
                token_id: row.token_id.clone(),
                name: row.name,
                symbol: row.symbol,
                image_uri: row.token_image_uri,
                description: row.description,
                is_listing: row.is_listing,
                twitter: row.twitter,
                telegram: row.telegram,
                website: row.website,
                created_at: row.created_at,
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
            market_info: MarketInfo {
                market_type: match row.market_type.as_str() {
                    "CURVE" => MarketType::Curve,
                    "DEX" => MarketType::Dex,
                    _ => MarketType::Curve,
                },
                token_id: row.token_id,
                market_id,
                token_price: row.token_price.to_plain_string(),
                native_price: row.native_price.to_plain_string(),
                price: row.price.to_plain_string(),
                total_supply: row.total_supply.to_plain_string(),
                liquidity: row.liquidity.to_plain_string(),
                volume: row.volume.to_plain_string(),
            },
            percent,
        }
    }
}


fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn calculate_price_change_percent(start_price: &str, current_price: &str) -> Option<f64> {
    let start: f64 = start_price.parse().ok()?;
    let current: f64 = current_price.parse().ok()?;

    if (start - 0.0).abs() < f64::EPSILON {
        return None;
    }

    let change_percent = ((current - start) / start) * 100.0;
    Some(change_percent)
}
