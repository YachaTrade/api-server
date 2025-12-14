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
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) as count FROM admin WHERE account_id = $1")
                .bind(account_id)
                .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to check admin status: {}", err))?;

        Ok(result > 0)
    }

    pub async fn set_nsfw(&self, request: SetNsfwRequest) -> Result<CmsActionResponse> {
        measure_postgres!(
            "cms.set_nsfw",
            sqlx::query("UPDATE token SET is_nsfw = $1 WHERE token_id = $2")
                .bind(request.is_nsfw)
                .bind(&request.token_id)
                .execute(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to set nsfw: {}", err))?;

        Ok(CmsActionResponse { success: true })
    }

    pub async fn insert_trend(&self, request: InsertTrendRequest) -> Result<CmsActionResponse> {
        // Start transaction
        let mut tx = self.db.get_write_pool().begin().await
            .map_err(|err| anyhow!("Failed to start transaction: {}", err))?;

        // Delete all existing trends
        measure_postgres!(
            "cms.trend.delete_all",
            sqlx::query("DELETE FROM trend")
                .execute(&mut *tx)
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

            measure_postgres!(
                "cms.trend.insert",
                query_builder.execute(&mut *tx)
            )
            .map_err(|err| anyhow!("Failed to insert trends: {}", err))?;
        }

        // Commit transaction
        tx.commit().await
            .map_err(|err| anyhow!("Failed to commit transaction: {}", err))?;

        Ok(CmsActionResponse { success: true })
    }
}
