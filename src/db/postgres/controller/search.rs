use std::sync::Arc;

use crate::{
    db::postgres::PostgresDatabase,
    types::response::{SearchTokenResponse, SearchTokenRow, UserInfoResponse},
};
use anyhow::{Context, Result};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use tracing::info;
pub struct SearchController {
    pub db: Arc<PostgresDatabase>,
}

impl SearchController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        SearchController { db }
    }

    pub async fn search_order_tokens(&self, query: &str) -> Result<Vec<SearchTokenResponse>> {
        let rows = sqlx::query_as!(
            SearchTokenRow,
            r#"
                SELECT 
                    t.token_id as "token_id!",
                    t.name as "name!",
                    t.symbol as "symbol!",
                    t.image_uri as "image_uri!",
                    t.description as "description!",
                    t.created_at as "created_at",
                    COALESCE(trc.reply_count::TEXT, '0') as "reply_count!",
                    c.price::TEXT as "price!",
                    a.nickname as "user_nickname",
                    a.account_id as "user_account_id",
                    a.image_uri as "user_image_uri"
                FROM 
                    token t
                LEFT JOIN 
                    account a ON t.creator = a.account_id
                LEFT JOIN 
                    token_reply_count trc ON t.token_id = trc.token_id
                LEFT JOIN 
                    curve c ON t.token_id = c.token_id
                WHERE 
                    LOWER(t.name) LIKE $1 
                    OR LOWER(t.symbol) LIKE $1
                    OR LOWER(t.token_id) LIKE $1
                ORDER BY 
                    c.price DESC NULLS LAST,
                    t.created_at DESC
                LIMIT 50
            "#,
            query
        )
        .fetch_all(&self.db.pool)
        .await
        .context("Failed to search tokens")?;
        info!("token_responses: {:?}", rows);
        let tokens: Vec<SearchTokenResponse> = rows
            .into_par_iter()
            .map(|row| SearchTokenResponse {
                token_id: row.token_id,
                name: row.name,
                symbol: row.symbol,
                image_uri: row.image_uri,
                description: row.description,
                created_at: row.created_at,
                reply_count: row.reply_count,
                price: row.price,
                user_info: UserInfoResponse {
                    nickname: row.user_nickname,
                    account_id: row.user_account_id,
                    image_uri: row.user_image_uri,
                },
            })
            .collect();

        Ok(tokens)
    }
}
