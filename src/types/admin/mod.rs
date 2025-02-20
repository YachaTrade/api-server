use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::db::postgres::PostgresDatabase;
use anyhow::Result;
#[derive(Debug, Deserialize)]
pub struct DeleteTokenRequest {
    pub token_id: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteTokenResponse {
    pub token_id: String,
}

pub struct AdminController {
    pub postgres: Arc<PostgresDatabase>,
}

impl AdminController {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    pub async fn delete_token(&self, token_id: &str) -> Result<DeleteTokenResponse> {
        sqlx::query!(
            r#"
            DELETE FROM swap WHERE token_id = $1;
            DELETE FROM swap_count WHERE token_id = $1;
            DELETE FROM market WHERE token_id = $1;
            DELETE FROM thread WHERE token_id = $1;
            DELETE FROM thread_count WHERE token_id = $1;
            DELETE FROM chart WHERE token_id = $1;
            DELETE FROM position WHERE token_id = $1;
            DELETE FROM token_reply_count WHERE token_id = $1;
            DELETE FROM token WHERE token_id = $1;
            "#,
            token_id
        )
        .execute(self.postgres.get_write_pool())
        .await?;

        Ok(DeleteTokenResponse {
            token_id: token_id.to_string(),
        })
    }
}
