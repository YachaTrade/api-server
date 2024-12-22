use std::sync::Arc;

use crate::db::postgres::{model::Token, PostgresDatabase};

use anyhow::Result;

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
            SELECT * FROM token WHERE LOWER(token_id) = LOWER($1)
            "#,
            token_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?;

        Ok(token)
    }
}
