use std::sync::Arc;

use crate::db::postgres::{
    controller::account::AccountController, model::Account, PostgresDatabase,
};

use anyhow::Result;

use tracing::debug;

pub struct SessionController {
    pub db: Arc<PostgresDatabase>,
}

impl SessionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        SessionController { db }
    }

    pub async fn set_session(&self, session_id: &str, address: &str) -> Result<()> {
        debug!("Setting session: {} -> {}", session_id, address);
        let account_controller = AccountController::new(self.db.clone());

        let mut tx = self.db.pool.begin().await?;

        // 계정 가져오기 또는 생성
        let account = account_controller.get_or_create_account(address).await?;

        // 기존 세션 삭제 및 새 세션 삽입
        sqlx::query!(
            r#"
            INSERT INTO account_session (id, account_id)
            VALUES ($1, $2)
            ON CONFLICT (account_id) DO UPDATE
            SET id = EXCLUDED.id
            "#,
            session_id,
            account.account_id,
        )
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;

        Ok(())
    }

    pub async fn get_address_by_session_id(&self, session_id: &str) -> Result<String> {
        let session = sqlx::query!(
            r#"
            SELECT account_id FROM account_session WHERE id = $1
            "#,
            session_id
        )
        .fetch_one(&self.db.pool)
        .await?;
        Ok(session.account_id)
    }

    pub async fn delete_session_by_address(&self, address: &str) -> Result<()> {
        let result = sqlx::query!(
            r#"
            DELETE FROM account_session WHERE account_id = $1
            "#,
            address
        )
        .execute(&self.db.pool)
        .await?;

        let rows_affected = result.rows_affected();

        if rows_affected == 0 {
            debug!("Attempted to delete non-existent session: {}", address);
        } else {
            debug!("Successfully deleted session: {}", address);
        }
        Ok(())
    }

    pub async fn delete_session_by_id(&self, session_id: &str) -> Result<()> {
        let result = sqlx::query!(
            r#"
            DELETE FROM account_session WHERE id = $1
            "#,
            session_id
        )
        .execute(&self.db.pool)
        .await?;

        let rows_affected = result.rows_affected();

        if rows_affected == 0 {
            debug!("Attempted to delete non-existent session: {}", session_id);
        } else {
            debug!("Successfully deleted session: {}", session_id);
        }
        Ok(())
    }
}
