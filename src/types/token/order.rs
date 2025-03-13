use std::sync::Arc;

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{info::AccountInfo, pagination::PaginationParams},
};
use anyhow::{anyhow, Result};
use bigdecimal::BigDecimal;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum TokenOrderType {
    MarketCap,    // price * reserve_token
    CreationTime, // created_at
    LatestTrade,  //
}

impl TokenOrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenOrderType::MarketCap => "market_cap",
            TokenOrderType::CreationTime => "creation_time",
            TokenOrderType::LatestTrade => "latest_trade",
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
    pub reply_count: String,
    pub price: String, //market.price
    pub reserve_token: BigDecimal,
    pub created_at: i64,
    pub market_type: String,
    pub is_king: bool,
    pub score: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrderTokenRaw {
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
    pub reply_count: String,
    pub price: String,
    pub reserve_token: BigDecimal,
    pub is_king: bool,
    pub market_type: String,
    pub created_at: i64,
    pub score: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderToken {
    pub token_info: OrderTokenInfo,
    pub account_info: AccountInfo,
}
impl From<OrderTokenRaw> for OrderToken {
    fn from(row: OrderTokenRaw) -> Self {
        OrderToken {
            token_info: OrderTokenInfo {
                token_id: row.token_id,
                name: row.name,
                symbol: row.symbol,
                image_uri: row.token_image_uri,
                description: row.description.unwrap_or_default(),
                reply_count: row.reply_count,
                price: row.price,
                reserve_token: row.reserve_token,
                created_at: row.created_at,
                market_type: row.market_type,
                is_king: row.is_king,
                score: row.score,
            },
            account_info: AccountInfo {
                account_id: row.account_id,
                image_uri: row.account_image_uri,
                nickname: row.nickname,
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
        let offset = (pagination.page - 1) * pagination.limit;
        let order_token_raw = match order_by {
            TokenOrderType::CreationTime => {
                sqlx::query_as::<_, OrderTokenRaw>(
                    r#"
                   SELECT 
                        t.token_id, a.account_id, a.follower_count, a.following_count, 
                        a.nickname, a.image_uri as account_image_uri, t.name, t.symbol, 
                        t.image_uri as token_image_uri, t.description, 
                        COALESCE(trc.reply_count::TEXT, '0') as reply_count,
                        COALESCE(m.price::TEXT, '0') as price,
                        COALESCE(m.reserve_token, '0') as reserve_token,
                        COALESCE(k.token_id IS NOT NULL, false) as is_king,
                        m.market_type, t.created_at, t.created_at::FLOAT8 as score
                    FROM token t
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN LATERAL (
                        SELECT token_id, reply_count 
                        FROM token_reply_count 
                        WHERE token_id = t.token_id
                    ) trc ON TRUE
                    LEFT JOIN LATERAL (
                        SELECT token_id, price, reserve_token, market_type 
                        FROM market 
                        WHERE token_id = t.token_id
                    ) m ON TRUE
                    LEFT JOIN LATERAL (
                        SELECT token_id 
                        FROM king 
                        WHERE token_id = t.token_id
                    ) k ON TRUE
                    ORDER BY t.created_at DESC
                    LIMIT $1 OFFSET $2
                    "#,
                )
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(&*self.db.get_read_pool())
                .await?
            }
            TokenOrderType::LatestTrade => {
                sqlx::query_as::<_, OrderTokenRaw>(
                    r#"
                    WITH latest_swap_times AS (
                            SELECT DISTINCT ON (token_id) 
                                token_id, created_at
                            FROM swap
                            ORDER BY token_id, created_at DESC
                    )
                    SELECT 
                        t.token_id, a.account_id, a.nickname, a.image_uri as account_image_uri,
                        a.follower_count, a.following_count, t.name, t.symbol,
                        t.image_uri as token_image_uri, t.description,
                        COALESCE(trc.reply_count::TEXT, '0') as reply_count,
                        COALESCE(m.price::TEXT, '0') as price,
                        COALESCE(m.reserve_token, '0') as reserve_token,
                        COALESCE(k.token_id IS NOT NULL, false) as is_king,
                        m.market_type, t.created_at, lst.created_at::FLOAT8 as score
                    FROM (
                        SELECT * FROM latest_swap_times
                        ORDER BY created_at DESC
                        LIMIT $1 OFFSET $2
                    ) lst
                    JOIN token t ON lst.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN token_reply_count trc ON t.token_id = trc.token_id
                    LEFT JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN king k ON t.token_id = k.token_id
                    ORDER BY score DESC
                    "#,
                )
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(&*self.db.get_read_pool())
                .await?
            }
            TokenOrderType::MarketCap => {
                sqlx::query_as::<_, OrderTokenRaw>(
                    r#"
                   SELECT 
                        t.token_id, a.account_id, a.nickname, a.image_uri as account_image_uri,
                        a.follower_count, a.following_count, t.name, t.symbol,
                        t.image_uri as token_image_uri, t.description,
                        COALESCE(trc.reply_count::TEXT, '0') as reply_count,
                        COALESCE(m.price::TEXT, '0') as price,
                        COALESCE(m.reserve_token, '0') as reserve_token,
                        COALESCE(k.token_id IS NOT NULL, false) as is_king,
                        m.market_type, t.created_at, m.price::FLOAT8 as score
                    FROM (
                        SELECT token_id, price, reserve_token, market_type
                        FROM market
                        ORDER BY price DESC
                        LIMIT $1 OFFSET $2
                    ) m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN token_reply_count trc ON t.token_id = trc.token_id
                    LEFT JOIN king k ON t.token_id = k.token_id
                    ORDER BY score DESC
                    "#,
                )
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(&*self.db.get_read_pool())
                .await?
            }
        };

        let tokens: Vec<OrderToken> = order_token_raw.into_iter().map(OrderToken::from).collect();
        Ok(tokens)
    }

    pub async fn get_latest_king_of_the_hill(&self) -> Result<Option<OrderToken>> {
        let row = sqlx::query_as::<_, OrderTokenRaw>(
            r#"
            WITH latest_king AS (
                SELECT token_id, created_at
                FROM king
                WHERE created_at = (SELECT MAX(created_at) FROM king)
            )
            SELECT 
                t.token_id,
                a.account_id,
                a.nickname,
                a.image_uri as account_image_uri,
                a.follower_count as follower_count,
                a.following_count as following_count,
                t.name,
                t.symbol,
                t.image_uri as token_image_uri,
                t.description,
                COALESCE(trc.reply_count::TEXT, '0') as reply_count,
                COALESCE(m.price::TEXT, '0') as price,
                COALESCE(m.reserve_token, '0') as reserve_token,
                COALESCE(lk.token_id IS NOT NULL, false) as is_king,
                lk.created_at as is_king_created_at,
                m.market_type,
                t.created_at as created_at,
                COALESCE(lk.created_at::FLOAT8, 0) as score
            FROM latest_king lk
            JOIN token t ON t.token_id = lk.token_id
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN token_reply_count trc ON t.token_id = trc.token_id
            LEFT JOIN market m ON t.token_id = m.token_id
            "#,
        )
        .fetch_optional(self.db.get_read_pool())
        .await
        .map_err(|e| anyhow!("Failed to get king: {}", e))?;

        Ok(row.map(OrderToken::from))
    }

    pub async fn get_total_count(&self) -> Result<i64> {
        let row = sqlx::query!(
            r#"
            SELECT count
            FROM token_count
            "#,
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .map_err(|e| anyhow!("Failed to get token count: {}", e))?;

        Ok(row.count)
    }
}
