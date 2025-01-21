use std::sync::Arc;

use crate::{
    db::postgres::PostgresDatabase,
    types::{
        order::TokenOrderType,
        pagination::PaginationParams,
        response::{AccountInfo, OrderToken, OrderTokenRaw, TokenInfo},
    },
};
use anyhow::{anyhow, Result};
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use tracing::info;

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
        pagination: PaginationParams,
    ) -> Result<Vec<OrderToken>> {
        let offset = (pagination.page - 1) * pagination.limit;
        let order_token_raw = match order_by {
            TokenOrderType::CreationTime => {
                sqlx::query_as::<_, OrderTokenRaw>(
                    r#"
                    SELECT 
                        t.token_id as token_id,
                        a.account_id as account_id,
                        a.nickname,
                        a.image_uri as account_image_uri,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        COALESCE(trc.reply_count::TEXT, '0') as reply_count,
                        COALESCE(m.price::TEXT, '0') as price,
                        COALESCE(m.reserve_token::TEXT, '0') as reserve_token,
                        COALESCE(k.token_id IS NOT NULL, false) as is_king,
                        m.market_type,
                        t.created_at as created_at,
                        t.created_at::FLOAT8 as score
                    FROM token t
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN token_reply_count trc ON t.token_id = trc.token_id
                    LEFT JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN king k ON t.token_id = k.token_id
                    ORDER BY score DESC
                    LIMIT $1
                    OFFSET $2
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
                    WITH latest_swaps AS (
                        SELECT token_id, MAX(created_at) as created_at
                        FROM swap
                        GROUP BY token_id
                        ORDER BY MAX(created_at) DESC
                        LIMIT $1  -- 필요한 만큼만!
                        OFFSET $2
                    )   
                    SELECT 
                        t.token_id as token_id,
                        a.account_id as account_id,
                        a.nickname,
                        a.image_uri as account_image_uri,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        COALESCE(trc.reply_count::TEXT, '0') as reply_count,
                        COALESCE(m.price::TEXT, '0') as price,
                        COALESCE(m.reserve_token::TEXT, '0') as reserve_token,
                        COALESCE(k.token_id IS NOT NULL, false) as is_king,
                        m.market_type,
                        t.created_at as created_at,
                        ls.created_at::FLOAT8 as score

                    FROM (
                        SELECT DISTINCT ON (token_id) *
                        FROM latest_swaps
                        ORDER BY token_id, created_at DESC
                        LIMIT 50
                    ) ls
                    JOIN token t ON ls.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN token_reply_count trc ON t.token_id = trc.token_id
                    LEFT JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN king k ON t.token_id = k.token_id
                    ORDER BY ls.created_at DESC
                    LIMIT $1
                    OFFSET $2
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
                    SELECT DISTINCT ON (m.token_id)
                        t.token_id as token_id,
                        a.account_id as account_id,
                        a.nickname,
                        a.image_uri as account_image_uri,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        COALESCE(trc.reply_count::TEXT, '0') as reply_count,
                        COALESCE(m.price::TEXT, '0') as price,
                        COALESCE(m.reserve_token::TEXT, '0') as reserve_token,
                        COALESCE(k.token_id IS NOT NULL, false) as is_king,
                        m.market_type,
                        t.created_at as created_at,
                        m.price::FLOAT8 as score
                    FROM market m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN token_reply_count trc ON t.token_id = trc.token_id
                    LEFT JOIN king k ON t.token_id = k.token_id
                    ORDER BY m.token_id, score DESC
                    LIMIT $1
                    OFFSET $2
                    "#,
                )
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(&*self.db.get_read_pool())
                .await?
            }
            TokenOrderType::ReplyCount => {
                sqlx::query_as::<_, OrderTokenRaw>(
                    r#"
                SELECT 
                    t.token_id as token_id,
                    a.account_id as account_id,
                    a.nickname,
                    a.image_uri as account_image_uri,
                    t.name,
                    t.symbol,
                    t.image_uri as token_image_uri,
                    t.description,
                    COALESCE(trc.reply_count::TEXT, '0') as reply_count,
                    COALESCE(m.price::TEXT, '0') as price,
                    COALESCE(m.reserve_token::TEXT, '0') as reserve_token,
                    COALESCE(k.token_id IS NOT NULL, false) as is_king,
                    m.market_type,
                    t.created_at as created_at,
                    COALESCE(trc.reply_count::FLOAT8, 0) as score
                FROM token t
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN token_reply_count trc ON t.token_id = trc.token_id
                LEFT JOIN market m ON t.token_id = m.token_id
                LEFT JOIN king k ON t.token_id = k.token_id
                ORDER BY score DESC NULLS LAST
                LIMIT $1
                OFFSET $2
                    "#,
                )
                .bind(pagination.limit)
                .bind(offset)
                .fetch_all(&*self.db.get_read_pool())
                .await?
            }
            TokenOrderType::LatestReply => {
                sqlx::query_as::<_, OrderTokenRaw>(
                    r#"
                    SELECT 
                        t.token_id as token_id,
                        a.account_id as account_id,
                        a.nickname,
                        a.image_uri as account_image_uri,
                        t.name,
                        t.symbol,
                        t.image_uri as token_image_uri,
                        t.description,
                        COALESCE(trc.reply_count::TEXT, '0') as reply_count,
                        COALESCE(m.price::TEXT, '0') as price,
                        COALESCE(m.reserve_token::TEXT, '0') as reserve_token,
                        COALESCE(k.token_id IS NOT NULL, false) as is_king,
                        m.market_type,
                        t.created_at as created_at,
                        th.created_at::FLOAT8 as score
                    FROM token t
                    JOIN account a ON t.creator = a.account_id
                    JOIN thread th ON t.token_id = th.token_id  
                    LEFT JOIN token_reply_count trc ON t.token_id = trc.token_id
                    LEFT JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN king k ON t.token_id = k.token_id
                    ORDER BY score DESC
                    LIMIT $1
                    OFFSET $2
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
    pub async fn search_order_tokens(
        &self,
        query: &str,
        order_by: TokenOrderType,
    ) -> Result<Vec<OrderToken>> {
        let search_pattern = format!("%{}%", query.to_lowercase());
        info!("Search pattern: {}", search_pattern);

        let order_by = match order_by {
            TokenOrderType::MarketCap => "m.price DESC NULLS LAST",
            TokenOrderType::CreationTime => "t.created_at DESC",
            TokenOrderType::LatestTrade => "m.price DESC NULLS LAST", //unused default marketcap
            TokenOrderType::ReplyCount => "m.price DESC NULLS LAST",  //unused default marketcap
            TokenOrderType::LatestReply => "m.price DESC NULLS LAST", //unused default marketcap
        };

        let query = format!(
            r#"
            SELECT 
                t.token_id,
                a.account_id,
                a.nickname,
                a.image_uri as account_image_uri,
                t.name,
                t.symbol,
                t.image_uri as token_image_uri,
                t.description,
                COALESCE(trc.reply_count::TEXT, '0') as reply_count,
                COALESCE(m.price::TEXT, '0') as price,
                COALESCE(m.reserve_token::TEXT, '0') as reserve_token,
                COALESCE(k.token_id IS NOT NULL, false) as is_king,
                m.market_type,
                t.created_at,
                COALESCE(trc.reply_count::FLOAT8, 0) as score
            FROM token t
            JOIN account a ON t.creator = a.account_id
            LEFT JOIN token_reply_count trc ON t.token_id = trc.token_id
            LEFT JOIN market m ON t.token_id = m.token_id
            LEFT JOIN king k ON t.token_id = k.token_id
            WHERE 
                LOWER(t.token_id) LIKE $1
                OR LOWER(t.name) LIKE $1 
                OR LOWER(t.symbol) LIKE $1
            ORDER BY {order_by}
            LIMIT 50
            "#
        );

        let rows = sqlx::query_as::<_, OrderTokenRaw>(&query)
            .bind(search_pattern)
            .fetch_all(&*self.db.get_read_pool())
            .await
            .map_err(|err| anyhow!("Failed to search tokens: {}", err))?;

        let tokens: Vec<OrderToken> = rows
            .into_par_iter()
            .map(|row| OrderToken {
                account_info: AccountInfo {
                    nickname: row.nickname,
                    account_id: row.account_id,
                    image_uri: row.account_image_uri,
                },
                token_info: TokenInfo {
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
            })
            .collect();

        Ok(tokens)
    }
}
