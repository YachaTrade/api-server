use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::{CountRow, info::{AccountInfo, MarketInfo, MarketType, TokenInfo}, pagination::PaginationParams},
        token::order::{OrderMessage, OrderToken, TokenOrderType},
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
                        t.is_listing,
                        t.created_at,
                        t.creator,
                        COALESCE(
                            CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                            a.nickname
                        ) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        a.follower_count as creator_follower_count,
                        a.following_count as creator_following_count,
                        m.market_type,
                        m.market_id,
                        (m.price * COALESCE(p.price, 0)) as token_price,
                        COALESCE(p.price, 0) as native_price,
                        m.price,
                        t.total_supply
                    FROM token t
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN LATERAL (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    ) p ON true
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
                        .fetch_all(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by creation time: {}", err))?
            }
            TokenOrderType::LatestTrade => {
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
                        t.is_listing,
                        t.created_at,
                        t.creator,
                        COALESCE(
                            CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                            a.nickname
                        ) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        a.follower_count as creator_follower_count,
                        a.following_count as creator_following_count,
                        m.market_type,
                        m.market_id,
                        (m.price * COALESCE(p.price, 0)) as token_price,
                        COALESCE(p.price, 0) as native_price,
                        m.price,
                        t.total_supply
                    FROM market m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    LEFT JOIN LATERAL (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    ) p ON true
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
                        .fetch_all(&*self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by latest trade: {}", err))?
            }
            TokenOrderType::MarketCap => {
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
                        t.is_listing,
                        t.created_at,
                        t.creator,
                        COALESCE(
                            CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                            a.nickname
                        ) as creator_nickname,
                        a.bio as creator_bio,
                        COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                        a.follower_count as creator_follower_count,
                        a.following_count as creator_following_count,
                        m.market_type,
                        m.market_id,
                        (m.price * COALESCE(p.price, 0)) as token_price,
                        COALESCE(p.price, 0) as native_price,
                        m.price,
                        t.total_supply
                    FROM (
                        SELECT token_id, price, market_type, market_id
                        FROM market
                        ORDER BY price {}
                        LIMIT $1 OFFSET $2
                    ) m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    LEFT JOIN LATERAL (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    ) p ON true
                    ORDER BY m.price {}
                    "#,
                    order_direction, order_direction
                );

                measure_postgres!(
                    "token_order.fetch_market_cap",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit)
                        .bind(offset)
                        .fetch_all(&*self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch tokens by market cap: {}", err))?
            }
            TokenOrderType::Verified => {
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
                        t.is_listing,
                        t.created_at,
                        t.creator,
                        REPLACE(ax.x_handle, '@', '#') as creator_nickname,
                        a.bio as creator_bio,
                        ax.x_image_uri as creator_image_uri,
                        a.follower_count as creator_follower_count,
                        a.following_count as creator_following_count,
                        m.market_type,
                        m.market_id,
                        (m.price * COALESCE(p.price, 0)) as token_price,
                        COALESCE(p.price, 0) as native_price,
                        m.price,
                        t.total_supply
                    FROM account_verified av
                    JOIN account_x ax ON av.x_handle = ax.x_handle
                    JOIN account a ON ax.account_id = a.account_id
                    JOIN token t ON t.creator = a.account_id
                    JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN LATERAL (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    ) p ON true
                    ORDER BY m.price {}
                    LIMIT $1 OFFSET $2
                    "#,
                    order_direction
                );

                measure_postgres!(
                    "token_order.fetch_verified",
                    sqlx::query_as::<_, OrderTokenRow>(&query)
                        .bind(pagination.limit)
                        .bind(offset)
                        .fetch_all(&*self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to fetch verified tokens: {}", err))?
            }
        };

        Ok(rows.into_iter().map(OrderToken::from).collect())
    }

    pub async fn get_latest_king_of_the_hill(&self) -> Result<Option<OrderToken>> {
        let cache_key = "king_of_the_hill:latest";

        let king = with_cache(&GLOBAL_CACHE.cache, cache_key, || async {
            self.fetch_latest_king_of_the_hill().await
        })
        .await?;

        Ok(king)
    }

    async fn fetch_latest_king_of_the_hill(&self) -> Result<Option<OrderToken>> {
        let row = measure_postgres!(
            "token_order.fetch_latest_king",
            sqlx::query_as::<_, OrderTokenRow>(
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
                    t.is_listing,
                    t.created_at,
                    t.creator,
                    COALESCE(
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END,
                        a.nickname
                    ) as creator_nickname,
                    a.bio as creator_bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    a.follower_count as creator_follower_count,
                    a.following_count as creator_following_count,
                    COALESCE(m.market_type, 'CURVE') as market_type,
                    COALESCE(m.market_id, '') as market_id,
                    (COALESCE(m.price, 0) * COALESCE(p.price, 0)) as token_price,
                    COALESCE(p.price, 0) as native_price,
                    COALESCE(m.price, 0) as price,
                    COALESCE(t.total_supply, 0) as total_supply
                FROM king k
                JOIN token t ON t.token_id = k.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                LEFT JOIN market m ON t.token_id = m.token_id
                LEFT JOIN LATERAL (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                ) p ON true
                WHERE k.created_at = (SELECT MAX(created_at) FROM king)
                "#,
            )
            .fetch_optional(self.db.get_read_pool())
        )
        .map_err(|e| anyhow!("Failed to get king: {}", e))?;

        Ok(row.map(OrderToken::from))
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

    pub fn build_order_message(
        order_type: TokenOrderType,
        order_tokens: Option<Vec<OrderToken>>,
        king_of_the_hill: Option<OrderToken>,
        total_count: i64,
    ) -> OrderMessage {
        OrderMessage {
            order_type,
            order_token: order_tokens,
            king_of_the_hill,
            total_count,
        }
    }
}

impl From<OrderTokenRow> for OrderToken {
    fn from(row: OrderTokenRow) -> Self {
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
                market_id: row.market_id,
                token_price: row.token_price.to_plain_string(),
                native_price: row.native_price.to_plain_string(),
                price: row.price.to_plain_string(),
                total_supply: row.total_supply.to_plain_string(),
            },
        }
    }
}
