use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::db::postgres::PostgresDatabase;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use tracing::info;
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
// TODO : nocne -> message 로 변경
#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "nonce": "example.com wants you to sign in with your Ethereum account:\n0x0000000000000000000000000000000000000000\n\nURI: https://example.com/login\nVersion: 1\nChain ID: 1\nNonce: abced-abced-abced\nIssued At: 2023-01-01T00:00:00Z"
}))]
pub struct AuthNonceResponse {
    #[schema(
        example = "example.com wants you to sign in with your Ethereum account:\n0x0000000000000000000000000000000000000000\n\nURI: https://example.com/login\nVersion: 1\nChain ID: 1\nNonce: abced-abced-abced\nIssued At: 2023-01-01T00:00:00Z"
    )]
    pub nonce: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AuthSessionRequest {
    pub signature: String,
    pub nonce: String,
    pub chain_id: u64,
    pub wallet_address: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthSessionResponse {
    pub account: Account,
}

#[derive(Debug, FromRow)]
struct SessionRow {
    account_id: String,
}

pub struct SessionController {
    pub db: Arc<PostgresDatabase>,
}

impl SessionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        SessionController { db }
    }

    pub async fn set_session(&self, session_id: &str, address: &str) -> Result<()> {
        let start_time = Instant::now();

        // 기존 세션 삭제 및 새 세션 삽입
        tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query(
                r#"
                INSERT INTO account_session (id, account_id)
                VALUES ($1, $2)
                ON CONFLICT (account_id) DO UPDATE
                SET id = EXCLUDED.id
                "#,
            )
            .bind(session_id)
            .bind(address)
            .execute(self.db.get_write_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        let elapsed = start_time.elapsed();
        info!("set_session(session_id: {}, address: {}) completed in {:?}", session_id, address, elapsed);

        Ok(())
    }

    pub async fn get_address_by_session_id(&self, session_id: &str) -> Result<String> {
        let start_time = Instant::now();

        let session = tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query_as::<_, SessionRow>(
                r#"
                SELECT account_id FROM account_session WHERE id = $1
                "#,
            )
            .bind(session_id)
            .fetch_one(self.db.get_read_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        let elapsed = start_time.elapsed();
        info!("get_address_by_session_id(session_id: {}) completed in {:?}", session_id, elapsed);

        Ok(session.account_id)
    }

    pub async fn delete_session_by_id(&self, session_id: &str) -> Result<()> {
        let start_time = Instant::now();

        tokio::time::timeout(
            Duration::from_millis(1000),
            sqlx::query(
                r#"
                DELETE FROM account_session WHERE id = $1
                "#,
            )
            .bind(session_id)
            .execute(self.db.get_write_pool()),
        )
        .await
        .map_err(|_| anyhow!("Query timeout after 1000ms"))??;

        let elapsed = start_time.elapsed();
        info!("delete_session_by_id(session_id: {}) completed in {:?}", session_id, elapsed);

        Ok(())
    }
}
