use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    config::{V1_BONDING_CURVE, V2_BONDING_CURVE},
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::{
            CountRow,
            info::{AccountInfo, MarketInfo, MarketType, TokenInfo, TokenVersion},
            pagination::PaginationParams,
        },
        token::order::{OrderToken, OrderTokenResponse, TokenOrderType},
    },
    utils::{
        calculate_price_change_percent, current_unix_timestamp,
        single_flight::{GLOBAL_CACHE, with_cache},
    },
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
    is_graduated: bool,
    is_nsfw: bool,
    is_cto: bool,
    version: TokenVersion,
    created_at: i64,
    creator: String,
    holder_count: i64,
    creator_nickname: String,
    creator_bio: String,
    creator_image_uri: String,
    market_type: String,
    market_id: String,
    quote_id: String,
    token_price: BigDecimal,
    native_price: BigDecimal,
    price: BigDecimal,
    price_usd: BigDecimal,
    total_supply: BigDecimal,
    reserve_quote: BigDecimal,
    reserve_token: BigDecimal,
    volume: BigDecimal,
    ath_price: BigDecimal,
    ath_price_quote: BigDecimal,
    price_24h_ago: BigDecimal,
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
        is_nsfw: bool,
    ) -> Result<Vec<OrderToken>> {
        let cache_key = cache_key!(
            "order_tokens",
            order_by.as_str(),
            pagination.direction,
            pagination.page,
            pagination.limit,
            is_nsfw
        );

        let order_by_clone = order_by;
        let tokens = with_cache(&GLOBAL_CACHE.cache, &cache_key, || async move {
            self.fetch_order_tokens(order_by_clone, pagination, is_nsfw)
                .await
        })
        .await?;

        Ok(tokens)
    }

    async fn fetch_order_tokens(
        &self,
        order_by: TokenOrderType,
        pagination: &PaginationParams,
        is_nsfw: bool,
    ) -> Result<Vec<OrderToken>> {
        let offset = (pagination.page - 1) * pagination.limit;
        let order_direction = &pagination.direction;

        let rows = match order_by {
            TokenOrderType::CreationTime => {
                let current_time = current_unix_timestamp();
                let time_24h_ago = current_time - 86400;

                let nsfw_filter = if is_nsfw { "TRUE" } else { "t.is_nsfw = false" };

                let query = format!(
                    r#"
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_graduated,
                        t.is_nsfw,
                        t.is_cto,
                        t.version,
                        t.created_at,
                        t.creator,
                        t.token_holder_count as holder_count,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        m.market_type,
                        COALESCE(m.pool_id, '') as market_id,
                    COALESCE(m.quote_id, '') as quote_id,
                        (m.price * COALESCE(lp.price, 0)) as token_price,
                        COALESCE(lp.price, 0) as native_price,
                        m.price,
                        (m.price * COALESCE(lp.price, 0)) as price_usd,
                        t.total_supply,
                        COALESCE(m.reserve_quote, 0) as reserve_quote,
                        COALESCE(m.reserve_token, 0) as reserve_token,
                        m.volume,
                        m.ath_price,
                        m.ath_price_quote,
                        COALESCE(
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                AND ph.created_at <= $3
                                ORDER BY
                                    ph.created_at DESC,
                                    ph.tx_index DESC,
                                    ph.log_index DESC
                                LIMIT 1
                            ),
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                ORDER BY
                                    ph.created_at ASC,
                                    ph.tx_index ASC,
                                    ph.log_index ASC
                                LIMIT 1
                            )
                        ) as price_24h_ago
                    FROM token t
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN LATERAL (
                        SELECT p.price
                        FROM price p
                        WHERE p.quote_id = m.quote_id
                        ORDER BY p.block_number DESC
                        LIMIT 1
                    ) lp ON true
                    WHERE {}
                    ORDER BY t.created_at {}
                    LIMIT $1 OFFSET $2
                    "#,
                    nsfw_filter, order_direction
                );

                measure_postgres!(
                    "token_order.fetch_creation_time",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit)
                        .bind(offset)
                        .bind(time_24h_ago)
                        .fetch_all(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by creation time: {}", err))?
            }
            TokenOrderType::LatestTrade => {
                let current_time = current_unix_timestamp();
                let time_24h_ago = current_time - 86400;

                let nsfw_filter = if is_nsfw { "TRUE" } else { "t.is_nsfw = false" };

                let query = format!(
                    r#"
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_graduated,
                        t.is_nsfw,
                        t.is_cto,
                        t.version,
                        t.created_at,
                        t.creator,
                        t.token_holder_count as holder_count,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        m.market_type,
                        COALESCE(m.pool_id, '') as market_id,
                    COALESCE(m.quote_id, '') as quote_id,
                        (m.price * COALESCE(lp.price, 0)) as token_price,
                        COALESCE(lp.price, 0) as native_price,
                        m.price,
                        (m.price * COALESCE(lp.price, 0)) as price_usd,
                        t.total_supply,
                        COALESCE(m.reserve_quote, 0) as reserve_quote,
                        COALESCE(m.reserve_token, 0) as reserve_token,
                        m.volume,
                        m.ath_price,
                        m.ath_price_quote,
                        COALESCE(
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                AND ph.created_at <= $3
                                ORDER BY
                                    ph.created_at DESC,
                                    ph.tx_index DESC,
                                    ph.log_index DESC
                                LIMIT 1
                            ),
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                ORDER BY
                                    ph.created_at ASC,
                                    ph.tx_index ASC,
                                    ph.log_index ASC
                                LIMIT 1
                            )
                        ) as price_24h_ago
                    FROM market m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN LATERAL (
                        SELECT p.price
                        FROM price p
                        WHERE p.quote_id = m.quote_id
                        ORDER BY p.block_number DESC
                        LIMIT 1
                    ) lp ON true
                    WHERE {}
                    ORDER BY m.latest_trade_at {}
                    LIMIT $1 OFFSET $2
                    "#,
                    nsfw_filter, order_direction
                );

                measure_postgres!(
                    "token_order.fetch_latest_trade",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit)
                        .bind(offset)
                        .bind(time_24h_ago)
                        .fetch_all(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by latest trade: {}", err))?
            }
            TokenOrderType::MarketCap => {
                let current_time = current_unix_timestamp();
                let time_24h_ago = current_time - 86400;

                let nsfw_filter = if is_nsfw { "TRUE" } else { "t.is_nsfw = false" };

                let query = format!(
                    r#"
                    SELECT
                        t.token_id,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        t.twitter,
                        t.telegram,
                        t.website,
                        t.is_graduated,
                        t.is_nsfw,
                        t.is_cto,
                        t.version,
                        t.created_at,
                        t.creator,
                        t.token_holder_count as holder_count,
                        COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        m.market_type,
                        COALESCE(m.pool_id, '') as market_id,
                    COALESCE(m.quote_id, '') as quote_id,
                        (m.price * COALESCE(lp.price, 0)) as token_price,
                        COALESCE(lp.price, 0) as native_price,
                        m.price,
                        (m.price * COALESCE(lp.price, 0)) as price_usd,
                        t.total_supply,
                        COALESCE(m.reserve_quote, 0) as reserve_quote,
                        COALESCE(m.reserve_token, 0) as reserve_token,
                        m.volume,
                        m.ath_price,
                        m.ath_price_quote,
                        COALESCE(
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                AND ph.created_at <= $3
                                ORDER BY
                                    ph.created_at DESC,
                                    ph.tx_index DESC,
                                    ph.log_index DESC
                                LIMIT 1
                            ),
                            (
                                SELECT ph.price
                                FROM price_history ph
                                WHERE ph.token_id = t.token_id
                                ORDER BY
                                    ph.created_at ASC,
                                    ph.tx_index ASC,
                                    ph.log_index ASC
                                LIMIT 1
                            )
                        ) as price_24h_ago
                    FROM (
                        SELECT m.token_id, m.price, m.market_type, m.pool_id, m.quote_id, m.reserve_quote, m.reserve_token, m.volume, m.ath_price, m.ath_price_quote
                        FROM market m
                        JOIN token t ON m.token_id = t.token_id
                        WHERE {}
                        ORDER BY m.price {}
                        LIMIT $1 OFFSET $2
                    ) m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN LATERAL (
                        SELECT p.price
                        FROM price p
                        WHERE p.quote_id = m.quote_id
                        ORDER BY p.block_number DESC
                        LIMIT 1
                    ) lp ON true
                    ORDER BY m.price {}
                    "#,
                    nsfw_filter, order_direction, order_direction
                );

                measure_postgres!(
                    "token_order.fetch_market_cap",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit)
                        .bind(offset)
                        .bind(time_24h_ago)
                        .fetch_all(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by market cap: {}", err))?
            }
        };

        Ok(rows.into_iter().map(OrderToken::from).collect())
    }

    pub async fn get_total_count_by_type(
        &self,
        order_type: &TokenOrderType,
        is_nsfw: bool,
    ) -> Result<i64> {
        match order_type {
            _ => {
                // is_nsfw = true: return all tokens (total_count)
                // is_nsfw = false: return only SFW tokens (sfw_count)
                let column = if is_nsfw { "total_count" } else { "sfw_count" };
                let query = format!("SELECT {} as count FROM token_count", column);

                let row = measure_postgres!(
                    "token_order.get_total_count_by_type",
                    sqlx::query_as::<_, CountRow>(&query).fetch_one(self.db.get_read_pool())
                )
                .map_err(|e| anyhow!("Failed to get total_count: {}", e))?;

                Ok(row.count)
            }
        }
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
        if market_id.is_empty() {
            if row.market_type == "CURVE" {
                market_id = V1_BONDING_CURVE.clone();
            } else if row.market_type == "V2_CURVE" {
                market_id = V2_BONDING_CURVE.clone();
            }
        }

        let percent = calculate_price_change_percent(
            &row.price_24h_ago.normalized().to_plain_string(),
            &row.price.normalized().to_plain_string(),
        )
        .unwrap_or(0.0);

        OrderToken {
            token_info: TokenInfo {
                token_id: row.token_id.clone(),
                name: row.name,
                symbol: row.symbol,
                image_uri: row.token_image_uri,
                description: row.description,
                is_graduated: row.is_graduated,
                is_nsfw: row.is_nsfw,
                twitter: row.twitter,
                telegram: row.telegram,
                website: row.website,
                created_at: row.created_at,
                creator: AccountInfo {
                    account_id: row.creator,
                    nickname: row.creator_nickname,
                    bio: row.creator_bio,
                    image_uri: row.creator_image_uri,
                },
                is_cto: row.is_cto,
                version: row.version.clone(),
            },
            market_info: MarketInfo {
                market_type: match row.market_type.as_str() {
                    "CURVE" => MarketType::Curve,
                    "DEX" => MarketType::Dex,
                    "V2_CURVE" => MarketType::V2Curve,
                    "V2_DEX" => MarketType::V2Dex,
                    _ => MarketType::Curve,
                },
                token_id: row.token_id,
                quote_id: row.quote_id.clone(),
                market_id,
                token_price: row.token_price.normalized().to_plain_string(),
                native_price: row.native_price.normalized().to_plain_string(),
                quote_price: row.native_price.normalized().to_plain_string(),
                price: row.price.normalized().to_plain_string(),
                price_usd: row.price_usd.normalized().to_plain_string(),
                price_native: row.price.normalized().to_plain_string(),
                price_quote: row.price.normalized().to_plain_string(),
                total_supply: row.total_supply.normalized().to_plain_string(),
                reserve_native: row.reserve_quote.normalized().to_plain_string(),
                reserve_quote: row.reserve_quote.normalized().to_plain_string(),
                reserve_token: row.reserve_token.normalized().to_plain_string(),
                volume: row.volume.normalized().to_plain_string(),
                ath_price: row.ath_price.normalized().to_plain_string(),
                ath_price_usd: row.ath_price.normalized().to_plain_string(),
                ath_price_native: row.ath_price_quote.normalized().to_plain_string(),
                ath_price_quote: row.ath_price_quote.normalized().to_plain_string(),
                holder_count: row.holder_count,
            },
            percent,
        }
    }
}
