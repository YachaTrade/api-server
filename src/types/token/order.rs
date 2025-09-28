use std::sync::Arc;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::{CountRow, info::AccountInfo, pagination::PaginationParams},
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};
use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug, Copy, Clone, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TokenOrderType {
    MarketCap,    // price * reserve_token
    CreationTime, // created_at
    LatestTrade,  // market latest_trade_at
    Verified,     // verified
}

impl TokenOrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenOrderType::MarketCap => "market_cap",
            TokenOrderType::CreationTime => "creation_time",
            TokenOrderType::LatestTrade => "latest_trade",
            TokenOrderType::Verified => "verified",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct OrderTokenInfo {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: String,
    pub market_cap: String,
    pub reserve_token: String,
    pub created_at: i64,
    pub market_type: String,
    pub score: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrderTokenRow {
    pub token_id: String,
    pub account_id: String,
    pub nickname: String,
    pub follower_count: i32,
    pub following_count: i32,
    pub account_image_uri: String,
    pub name: String,
    pub symbol: String,
    pub token_image_uri: String,
    pub description: Option<String>,
    pub total_supply: BigDecimal,
    pub price: BigDecimal,
    pub reserve_token: BigDecimal,
    pub market_type: String,
    pub created_at: i64,
    pub score: f64,
    pub x_handle: Option<String>,
    pub x_image_uri: Option<String>,
    pub is_blue_label: Option<bool>,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderToken {
    pub token_info: OrderTokenInfo,
    pub account_info: AccountInfo,
}
impl From<OrderTokenRow> for OrderToken {
    fn from(row: OrderTokenRow) -> Self {
        OrderToken {
            token_info: OrderTokenInfo {
                token_id: row.token_id,
                name: row.name,
                symbol: row.symbol,
                image_uri: row.token_image_uri,
                description: row.description.unwrap_or_default(),
                market_cap: (row.total_supply * row.price).to_plain_string(),
                reserve_token: row.reserve_token.to_plain_string(),
                created_at: row.created_at,
                market_type: row.market_type,
                score: row.score,
            },
            account_info: AccountInfo {
                account_id: row.account_id,
                image_uri: if row.x_image_uri.is_some() {
                    row.x_image_uri.unwrap()
                } else {
                    row.account_image_uri
                },
                nickname: if row.x_handle.is_some() {
                    row.x_handle.unwrap()
                } else {
                    row.nickname
                },
                follower_count: row.follower_count,
                following_count: row.following_count,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderMessage {
    pub order_type: TokenOrderType,
    pub order_token: Option<Vec<OrderToken>>,
    pub king_of_the_hill: Option<OrderToken>,
    pub total_count: i64,
}

pub struct OrderController {
    pub db: Arc<PostgresDatabase>,
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
        // 캐시 키 생성
        let cache_key = cache_key!(
            "order_tokens",
            order_by.as_str(),
            pagination.direction,
            pagination.page,
            pagination.limit
        );

        // 모든 캐시는 1초로 통일 (Redis와 동일)
        let cache = &GLOBAL_CACHE.cache;

        // Single Flight Pattern: 동일한 요청은 하나의 Future를 공유
        let order_by_clone = order_by.clone();
        let tokens = with_cache(cache, &cache_key, || async move {
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
        let order_token_raw = match order_by {
            TokenOrderType::CreationTime => {
                let query = format!(
                    r#"
                     SELECT 
                        t.token_id, a.account_id, a.follower_count, a.following_count, 
                        a.nickname, a.image_uri as account_image_uri, t.name, t.symbol, 
                        t.image_uri as token_image_uri, t.description, 
                        t.total_supply as total_supply,
                        m.price,
                        m.reserve_token,
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                        ax.x_image_uri,
                        ax.is_blue_label,
                        m.market_type, t.created_at, t.created_at::FLOAT8 as score
                    FROM token t
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON t.creator = ax.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                    INNER JOIN market m ON t.token_id = m.token_id
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
                        t.token_id, a.account_id, a.nickname, a.image_uri as account_image_uri,
                        a.follower_count, a.following_count, t.name, t.symbol,
                        t.image_uri as token_image_uri, t.description,
                        t.total_supply as total_supply,
                        m.price,
                        m.reserve_token,
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                        ax.x_image_uri,
                        ax.is_blue_label,
                        m.market_type, t.created_at, m.latest_trade_at::FLOAT8 as score
                    FROM market m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON t.creator = ax.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
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
                        t.token_id, a.account_id, a.nickname, a.image_uri as account_image_uri,
                        a.follower_count, a.following_count, t.name, t.symbol,
                        t.image_uri as token_image_uri, t.description,
                        t.total_supply as total_supply,
                        m.price,
                        m.reserve_token,
                        CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                        ax.x_image_uri,
                        ax.is_blue_label,
                        m.market_type, t.created_at, m.price::FLOAT8 as score
                    FROM (
                        SELECT token_id, price, reserve_token, market_type
                        FROM market
                        ORDER BY price {}
                        LIMIT $1 OFFSET $2
                    ) m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON t.creator = ax.account_id
                    LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
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
                        t.token_id, a.account_id, a.nickname, a.image_uri as account_image_uri,
                        a.follower_count, a.following_count, t.name, t.symbol,
                        t.image_uri as token_image_uri, t.description,
                        t.total_supply as total_supply,
                        m.price,
                        m.reserve_token,
                        REPLACE(ax.x_handle, '@', '#') as x_handle,
                        ax.x_image_uri,
                        ax.is_blue_label,
                        m.market_type, t.created_at, m.price::FLOAT8 as score
                    FROM account_verified av
                    JOIN account_x ax ON av.x_handle = ax.x_handle
                    JOIN account a ON ax.account_id = a.account_id
                    JOIN token t ON t.creator = a.account_id
                    JOIN market m ON t.token_id = m.token_id
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

        let tokens: Vec<OrderToken> = order_token_raw.into_iter().map(OrderToken::from).collect();
        Ok(tokens)
    }

    pub async fn get_latest_king_of_the_hill(&self) -> Result<Option<OrderToken>> {
        let cache_key = "king_of_the_hill:latest";

        // Single Flight Pattern: 동일한 요청은 하나의 Future를 공유
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
                    a.account_id,
                    a.nickname,
                    a.image_uri as account_image_uri,
                    a.follower_count,
                    a.following_count,
                    t.name,
                    t.symbol,
                    t.image_uri as token_image_uri,
                    t.description,
                    t.total_supply as total_supply,
                    COALESCE(m.price, '0') as price,
                    COALESCE(m.reserve_token, '0') as reserve_token,
                    m.market_type,
                    t.created_at,
                    k.created_at::FLOAT8 as score,
                    CASE WHEN av.x_handle IS NOT NULL THEN REPLACE(ax.x_handle, '@', '#') ELSE ax.x_handle END as x_handle,
                    ax.x_image_uri,
                    ax.is_blue_label
                FROM king k
                JOIN token t ON t.token_id = k.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON t.creator = ax.account_id
                LEFT JOIN account_verified av ON ax.x_handle = av.x_handle
                LEFT JOIN market m ON t.token_id = m.token_id
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
}
