pub mod analytics;

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

    /// Single-statement CTE: pgbouncer runs in statement pooling mode, which
    /// forbids `BEGIN`/`COMMIT`. The admin check stays atomic with the mutation.
    pub async fn insert_trend_with_admin_check(
        &self,
        account_id: &str,
        request: InsertTrendRequest,
    ) -> Result<CmsActionResponse> {
        let is_admin: bool = measure_postgres!(
            "cms.trend.replace_all",
            sqlx::query_scalar(
                "WITH adm AS (
                     SELECT 1 AS ok FROM admin WHERE account_id = $2
                 ),
                 del AS (
                     DELETE FROM trend
                     WHERE EXISTS (SELECT 1 FROM adm) AND NOT (token_id = ANY($1::text[]))
                 ),
                 ins AS (
                     INSERT INTO trend (token_id, display_order)
                     SELECT u.token_id, (u.ord - 1)::int
                     FROM adm, unnest($1::text[]) WITH ORDINALITY AS u(token_id, ord)
                     ON CONFLICT (token_id) DO UPDATE
                       SET display_order = EXCLUDED.display_order,
                           created_at = EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)::BIGINT
                 )
                 SELECT EXISTS (SELECT 1 FROM adm) AS is_admin",
            )
            .bind(&request.token_ids)
            .bind(account_id)
            .fetch_one(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to replace trends: {}", err))?;

        if !is_admin {
            return Err(anyhow!("Admin access required"));
        }

        Ok(CmsActionResponse { success: true })
    }

    /// Verify admin status for update metadata operation
    pub async fn verify_admin(&self, account_id: &str) -> Result<bool> {
        let is_admin = measure_postgres!(
            "cms.verify_admin",
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM admin WHERE account_id = $1)"
            )
            .bind(account_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to verify admin status: {}", err))?;

        Ok(is_admin)
    }

    /// Update metadata_uri for a token
    pub async fn update_token_metadata_uri(
        &self,
        token_id: &str,
        metadata_uri: &str,
    ) -> Result<()> {
        measure_postgres!(
            "cms.update_token_metadata_uri",
            sqlx::query("UPDATE token SET metadata_uri = $1 WHERE token_id = $2")
                .bind(metadata_uri)
                .bind(token_id)
                .execute(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to update token metadata_uri: {}", err))?;

        Ok(())
    }

    /// Update token table metadata fields
    pub async fn update_token_metadata(
        &self,
        token_id: &str,
        description: Option<&str>,
        image_uri: Option<&str>,
        website: Option<&str>,
        twitter: Option<&str>,
        telegram: Option<&str>,
    ) -> Result<()> {
        measure_postgres!(
            "cms.update_token_metadata",
            sqlx::query(
                r#"
                UPDATE token SET
                    description = COALESCE($2, description),
                    image_uri = COALESCE($3, image_uri),
                    website = COALESCE($4, website),
                    twitter = COALESCE($5, twitter),
                    telegram = COALESCE($6, telegram)
                WHERE token_id = $1
                "#
            )
            .bind(token_id)
            .bind(description)
            .bind(image_uri)
            .bind(website)
            .bind(twitter)
            .bind(telegram)
            .execute(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to update token: {}", err))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const ADMIN: &str = "0x1111111111111111111111111111111111111111";
    const NON_ADMIN: &str = "0x2222222222222222222222222222222222222222";

    fn controller(pool: PgPool) -> CmsController {
        CmsController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn non_admin_trend_replace_leaves_existing_rows(pool: PgPool) {
        sqlx::query("INSERT INTO trend (token_id, display_order) VALUES ('0xOld', 0)")
            .execute(&pool)
            .await
            .unwrap();

        let error = controller(pool.clone())
            .insert_trend_with_admin_check(
                NON_ADMIN,
                InsertTrendRequest {
                    token_ids: vec!["0xNew".into()],
                },
            )
            .await
            .expect_err("non-admin must be rejected");
        assert!(error.to_string().contains("Admin access required"));

        let rows: Vec<(String, i32)> =
            sqlx::query_as("SELECT token_id, display_order FROM trend ORDER BY display_order")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rows, vec![("0xOld".into(), 0)]);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn admin_trend_replace_is_atomic_and_ordered(pool: PgPool) {
        sqlx::query("INSERT INTO admin (account_id) VALUES ($1)")
            .bind(ADMIN)
            .execute(&pool)
            .await
            .unwrap();
        let controller = controller(pool.clone());

        controller
            .insert_trend_with_admin_check(
                ADMIN,
                InsertTrendRequest {
                    token_ids: vec!["0xTokenA".into(), "0xTokenB".into()],
                },
            )
            .await
            .unwrap();
        controller
            .insert_trend_with_admin_check(
                ADMIN,
                InsertTrendRequest {
                    token_ids: vec!["0xTokenB".into()],
                },
            )
            .await
            .unwrap();

        let rows: Vec<(String, i32)> =
            sqlx::query_as("SELECT token_id, display_order FROM trend ORDER BY display_order")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(rows, vec![("0xTokenB".into(), 0)]);
    }
}
