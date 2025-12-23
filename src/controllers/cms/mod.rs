use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::cms::{CmsActionResponse, InsertTrendRequest, SetNsfwRequest},
};

pub struct CmsController {
    db: Arc<PostgresDatabase>,
}

impl CmsController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        CmsController { db }
    }

    pub async fn is_admin(&self, account_id: &str) -> Result<bool> {
        let result = measure_postgres!(
            "cms.is_admin",
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) as count FROM admin WHERE account_id = $1"
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to check admin status: {}", err))?;

        Ok(result > 0)
    }

    /// Set NSFW status with admin check in a single transaction to prevent TOCTOU attacks
    pub async fn set_nsfw_with_admin_check(
        &self,
        account_id: &str,
        request: SetNsfwRequest,
    ) -> Result<CmsActionResponse> {
        // Use a single query that checks admin and updates in one atomic operation
        let result = measure_postgres!(
            "cms.set_nsfw_with_admin_check",
            sqlx::query(
                r#"
                UPDATE token SET is_nsfw = $1
                WHERE token_id = $2
                AND EXISTS (SELECT 1 FROM admin WHERE account_id = $3)
                "#
            )
            .bind(request.is_nsfw)
            .bind(&request.token_id)
            .bind(account_id)
            .execute(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to set nsfw: {}", err))?;

        if result.rows_affected() == 0 {
            return Err(anyhow!("Admin access required or token not found"));
        }

        Ok(CmsActionResponse { success: true })
    }

    /// Insert trends with admin check in a single transaction to prevent TOCTOU attacks
    pub async fn insert_trend_with_admin_check(
        &self,
        account_id: &str,
        request: InsertTrendRequest,
    ) -> Result<CmsActionResponse> {
        // Start transaction
        let mut tx = self
            .db
            .get_write_pool()
            .begin()
            .await
            .map_err(|err| anyhow!("Failed to start transaction: {}", err))?;

        // Verify admin status with FOR UPDATE to lock the row during transaction
        let is_admin = measure_postgres!(
            "cms.trend.verify_admin",
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM admin WHERE account_id = $1 FOR UPDATE"
            )
            .bind(account_id)
            .fetch_one(&mut *tx)
        )
        .map_err(|err| anyhow!("Failed to verify admin status: {}", err))?;

        if is_admin == 0 {
            return Err(anyhow!("Admin access required"));
        }

        // Delete all existing trends
        measure_postgres!(
            "cms.trend.delete_all",
            sqlx::query("DELETE FROM trend").execute(&mut *tx)
        )
        .map_err(|err| anyhow!("Failed to delete trends: {}", err))?;

        // Insert new trends with order if not empty
        if !request.token_ids.is_empty() {
            let placeholders: Vec<String> = (0..request.token_ids.len())
                .map(|i| format!("(${}, ${})", i * 2 + 1, i * 2 + 2))
                .collect();

            let query = format!(
                "INSERT INTO trend (token_id, display_order) VALUES {}",
                placeholders.join(", ")
            );

            let mut query_builder = sqlx::query(&query);
            for (index, token_id) in request.token_ids.iter().enumerate() {
                query_builder = query_builder.bind(token_id).bind(index as i32);
            }

            measure_postgres!("cms.trend.insert", query_builder.execute(&mut *tx))
                .map_err(|err| anyhow!("Failed to insert trends: {}", err))?;
        }

        // Commit transaction
        tx.commit()
            .await
            .map_err(|err| anyhow!("Failed to commit transaction: {}", err))?;

        Ok(CmsActionResponse { success: true })
    }
}
