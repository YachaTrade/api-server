use std::sync::Arc;

use crate::db::postgres::PostgresDatabase;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::account::Account;

#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({
    "address": "Your address"
}))]
pub struct AuthNonceRequest {
    #[schema(example = "Your address")]
    pub address: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "nonce": "abced-abced-abced"
}))]
pub struct AuthNonceResponse {
    #[schema(example = "abced-abced-abced")]
    pub nonce: String,
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({
    "signature": "Signature with nonce signed with private key",
    "nonce": "Get nonce from /auth/nonce"
}))]
pub struct AuthSessionRequest {
    #[schema(example = "0x1234567890abcdef...")]
    pub signature: String,
    #[schema(example = "abcdef-abcedef-abcedf")]
    pub nonce: String,
    pub chain_id: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthSessionResponse {
    pub account: Account,
}

pub struct SessionController {
    pub db: Arc<PostgresDatabase>,
}

impl SessionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        SessionController { db }
    }

    pub async fn set_session(&self, session_id: &str, address: &str) -> Result<()> {
        // 기존 세션 삭제 및 새 세션 삽입
        sqlx::query!(
            r#"
            INSERT INTO account_session (id, account_id)
            VALUES ($1, $2)
            ON CONFLICT (account_id) DO UPDATE
            SET id = EXCLUDED.id
            "#,
            session_id,
            address
        )
        .execute(self.db.get_write_pool())
        .await?;

        Ok(())
    }

    pub async fn get_address_by_session_id(&self, session_id: &str) -> Result<String> {
        let session = sqlx::query!(
            r#"
            SELECT account_id FROM account_session WHERE id = $1
            "#,
            session_id
        )
        .fetch_one(self.db.get_read_pool())
        .await?;
        Ok(session.account_id)
    }

    pub async fn delete_session_by_address(&self, address: &str) -> Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM account_session WHERE account_id = $1
            "#,
            address
        )
        .execute(self.db.get_write_pool())
        .await?;

        Ok(())
    }

    pub async fn delete_session_by_id(&self, session_id: &str) -> Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM account_session WHERE id = $1
            "#,
            session_id
        )
        .execute(self.db.get_write_pool())
        .await?;

        Ok(())
    }
}
