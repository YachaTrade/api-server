use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{common::info::QuoteInfo, quote_token::QuoteTokensResponse},
};

#[derive(Debug, sqlx::FromRow)]
struct QuoteRow {
    quote_id: String,
    name: String,
    symbol: String,
    decimals: i32,
    image_uri: String,
}

pub struct QuoteTokenController {
    db: Arc<PostgresDatabase>,
}

impl QuoteTokenController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<QuoteTokensResponse> {
        let rows = measure_postgres!(
            "quote_token.list",
            sqlx::query_as::<_, QuoteRow>(
                r#"
                SELECT quote_id, name, symbol, decimals, image_uri
                FROM quote_token
                ORDER BY created_at ASC
                "#,
            )
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to list quote tokens: {}", err))?;

        let quote_tokens = rows
            .into_iter()
            .map(|r| QuoteInfo {
                quote_id: r.quote_id,
                name: r.name,
                symbol: r.symbol,
                decimals: r.decimals.max(0) as u32,
                image_uri: r.image_uri,
            })
            .collect();

        Ok(QuoteTokensResponse { quote_tokens })
    }
}
