use std::sync::Arc;

use crate::db::postgres::{model::Token, PostgresDatabase};

use anyhow::{anyhow, Context, Result};
use tracing::debug;
pub struct TokenController {
    pub db: Arc<PostgresDatabase>,
}

impl TokenController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        TokenController { db }
    }
    pub async fn get_token(&self, token_id: String) -> Result<Token> {
        let token = sqlx::query_as!(
            Token,
            r#"
            SELECT * FROM token WHERE token_id = $1
            "#,
            token_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?;

        Ok(token)
    }
    pub async fn get_token_tx(&self, tx: String) -> Result<Token> {
        let token = sqlx::query_as!(
            Token,
            r#"
            SELECT * FROM token WHERE create_transaction_hash = $1
            "#,
            tx
        )
        .fetch_one(self.db.get_read_pool())
        .await?;

        Ok(token)
    }
    pub async fn update_token_metadata(
        &self,
        transaction_hash: String,
        description: String,
        twitter: Option<String>,
        telegram: Option<String>,
        website: Option<String>,
        creator: String,
    ) -> Result<Token> {
        debug!("Update token Request by {}", creator);

        // Perform the update
        let token = sqlx::query_as!(
            Token,
            r#"
            UPDATE token
            SET description = $1,
                twitter = $2,
                telegram = $3,
                website = $4,
                is_updated = true
            WHERE create_transaction_hash = $5 AND creator = $6
            RETURNING *
            "#,
            description,
            twitter,
            telegram,
            website,
            transaction_hash,
            creator,
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .context("Failt Update token metadata")?;
        debug!("Updated token = {:?}", token);
        Ok(token)
    }
}
