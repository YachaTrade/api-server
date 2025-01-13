use std::sync::Arc;

use crate::{
    db::postgres::PostgresDatabase,
    types::response::{AccountInfo, SearchTokenInfo, SearchTokenRaw, SearchTokenResponse},
};
use anyhow::{anyhow, Result};
use rayon::iter::{IntoParallelIterator, ParallelIterator};

use serde::Deserialize;
use tracing::info;

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "snake_case")]
pub enum TokenSortBy {
    MarketCap,    // price * reserve_token
    CreationTime, // created_at
    LatestTrade,  //
    ReplyCount,
    LatestReply,
}

pub struct SearchController {
    pub db: Arc<PostgresDatabase>,
}

impl SearchController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        SearchController { db }
    }

    pub async fn search_order_tokens(
        &self,
        query: &str,
        sort_by: TokenSortBy,
    ) -> Result<Vec<SearchTokenResponse>> {
        let search_pattern = format!("%{}%", query.to_lowercase());
        info!("Search pattern: {}", search_pattern);

        let order_by = match sort_by {
            TokenSortBy::MarketCap => "m.price DESC NULLS LAST",
            TokenSortBy::CreationTime => "t.created_at DESC",
            TokenSortBy::LatestTrade => "m.price DESC NULLS LAST", //unused default marketcap
            TokenSortBy::ReplyCount => "m.price DESC NULLS LAST",  //unused default marketcap
            TokenSortBy::LatestReply => "m.price DESC NULLS LAST", //unused default marketcap
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

        let rows = sqlx::query_as::<_, SearchTokenRaw>(&query)
            .bind(search_pattern)
            .fetch_all(self.db.get_read_pool())
            .await
            .map_err(|err| anyhow!("Failed to search tokens: {}", err))?;

        let tokens: Vec<SearchTokenResponse> = rows
            .into_par_iter()
            .map(|row| SearchTokenResponse {
                account_info: AccountInfo {
                    nickname: row.nickname,
                    account_id: row.account_id,
                    image_uri: row.account_image_uri,
                },
                token_info: SearchTokenInfo {
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
