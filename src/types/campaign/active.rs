use std::sync::Arc;

use crate::db::postgres::PostgresDatabase;
use anyhow::Result;
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, Serialize, ToSchema)]
pub struct ActiveUserResponse {
    pub account: String,
    pub usage_checker: bool,
}

pub struct ActiveUserController {
    pub db: Arc<PostgresDatabase>,
}

impl ActiveUserController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        ActiveUserController { db }
    }

    pub async fn check_active_user(&self, account_id: &str) -> Result<ActiveUserResponse> {
        let exists: bool = sqlx::query_scalar!(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM active_users
                WHERE account_id = $1
            )
            "#,
            account_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?
        .unwrap_or(false); // None인 경우 false 반환

        Ok(ActiveUserResponse {
            account: account_id.to_string(),
            usage_checker: exists,
        })
    }
}
