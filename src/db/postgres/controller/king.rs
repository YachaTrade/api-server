use std::sync::Arc;

use crate::{
    db::postgres::PostgresDatabase,
    types::response::{OrderToken, OrderTokenRaw},
};

use anyhow::{anyhow, Result};

pub struct KingOfTheHillController {
    pub db: Arc<PostgresDatabase>,
}

impl KingOfTheHillController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        KingOfTheHillController { db }
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
                t.name,
                t.symbol,
                t.image_uri as token_image_uri,
                t.description,
                COALESCE(trc.reply_count::TEXT, '0') as reply_count,
                COALESCE(m.price::TEXT, '0') as price,
                COALESCE(m.reserve_token::TEXT, '0') as reserve_token,
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
}
