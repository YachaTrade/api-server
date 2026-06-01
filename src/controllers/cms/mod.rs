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

        // Verify admin status
        let is_admin = measure_postgres!(
            "cms.trend.verify_admin",
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM admin WHERE account_id = $1)"
            )
            .bind(account_id)
            .fetch_one(&mut *tx)
        )
        .map_err(|err| anyhow!("Failed to verify admin status: {}", err))?;

        if !is_admin {
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

    /// dex_token row 존재 여부 (이미지 업로드 前 404 판정용).
    /// write 경로 가드라 primary(write pool)에서 확인 — replica 복제 지연으로
    /// 방금 인덱서가 넣은 토큰을 못 보고 false 404를 내는 것을 막는다.
    pub async fn dex_token_exists(&self, token_id: &str) -> Result<bool> {
        let exists = measure_postgres!(
            "cms.dex_token_exists",
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM dex_token WHERE token_id = $1)"
            )
            .bind(token_id)
            .fetch_one(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to check dex_token existence: {}", err))?;

        Ok(exists)
    }

    /// admin 여부를 primary(write pool)에서 확인.
    /// write 작업 인가용 — read replica 복제 지연으로 권한 회수된 admin이 통과하는 것을 막는다.
    pub async fn verify_admin_on_writer(&self, account_id: &str) -> Result<bool> {
        let is_admin = measure_postgres!(
            "cms.verify_admin_on_writer",
            sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS(SELECT 1 FROM admin WHERE account_id = $1)"
            )
            .bind(account_id)
            .fetch_one(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to verify admin status: {}", err))?;

        Ok(is_admin)
    }

    /// dex_token.image_uri 갱신 — admin EXISTS를 같은 UPDATE에 원자적으로 묶어 인가.
    /// `verify_admin_on_writer` 이후 업로드 동안 권한이 회수되는 TOCTOU 창을 닫는다.
    /// row가 갱신됐으면 true (admin 아니거나 token 없으면 false).
    pub async fn set_dex_token_image(
        &self,
        account_id: &str,
        token_id: &str,
        image_uri: &str,
    ) -> Result<bool> {
        let result = measure_postgres!(
            "cms.set_dex_token_image",
            sqlx::query(
                r#"
                UPDATE dex_token SET image_uri = $1
                WHERE token_id = $2
                AND EXISTS (SELECT 1 FROM admin WHERE account_id = $3)
                "#
            )
            .bind(image_uri)
            .bind(token_id)
            .bind(account_id)
            .execute(self.db.get_write_pool())
        )
        .map_err(|err| anyhow!("Failed to set dex_token image: {}", err))?;

        Ok(result.rows_affected() > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    // 외부(PairCreated 발견) 토큰 — 베니티 접미사 없음, 체크섬 주소
    const DEX_TOKEN: &str = "0xA0b86991c6218B36c1D19d4A2E9eB0cE3606eB48";
    const ACC_ADMIN: &str = "0x1111111111111111111111111111111111111111";

    fn make_controller(pool: PgPool) -> CmsController {
        CmsController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn seed_dex_token(pool: &PgPool, token_id: &str, image_uri: &str) {
        sqlx::query(
            r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
               VALUES ($1, '', '', 18, $2, 0)
               ON CONFLICT (token_id) DO UPDATE SET image_uri = EXCLUDED.image_uri"#,
        )
        .bind(token_id)
        .bind(image_uri)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn dex_token_exists_true_when_present(pool: PgPool) {
        seed_dex_token(&pool, DEX_TOKEN, "").await;
        let ctrl = make_controller(pool);
        assert!(ctrl.dex_token_exists(DEX_TOKEN).await.unwrap());
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn dex_token_exists_false_when_absent(pool: PgPool) {
        let ctrl = make_controller(pool);
        assert!(!ctrl.dex_token_exists(DEX_TOKEN).await.unwrap());
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn set_dex_token_image_updates_existing_for_admin(pool: PgPool) {
        seed_dex_token(&pool, DEX_TOKEN, "old").await;
        seed_admin(&pool, ACC_ADMIN).await;
        let ctrl = make_controller(pool);
        let url = "https://storage.nadapp.net/coin/93c604c9-a488-4b0a-b988-79e1af171d5e";

        let updated = ctrl
            .set_dex_token_image(ACC_ADMIN, DEX_TOKEN, url)
            .await
            .unwrap();
        assert!(updated, "admin + existing row → updated");

        let stored: String =
            sqlx::query_scalar("SELECT image_uri FROM dex_token WHERE token_id = $1")
                .bind(DEX_TOKEN)
                .fetch_one(ctrl.db.get_read_pool())
                .await
                .unwrap();
        assert_eq!(stored, url);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn set_dex_token_image_false_when_not_admin(pool: PgPool) {
        seed_dex_token(&pool, DEX_TOKEN, "old").await;
        // ACC_ADMIN is NOT seeded into admin
        let ctrl = make_controller(pool);

        let updated = ctrl
            .set_dex_token_image(ACC_ADMIN, DEX_TOKEN, "new")
            .await
            .unwrap();
        assert!(!updated, "non-admin → no update (atomic guard)");

        let stored: String =
            sqlx::query_scalar("SELECT image_uri FROM dex_token WHERE token_id = $1")
                .bind(DEX_TOKEN)
                .fetch_one(ctrl.db.get_read_pool())
                .await
                .unwrap();
        assert_eq!(stored, "old", "image must be unchanged for non-admin");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn set_dex_token_image_false_when_token_absent(pool: PgPool) {
        seed_admin(&pool, ACC_ADMIN).await;
        let ctrl = make_controller(pool);
        let updated = ctrl
            .set_dex_token_image(ACC_ADMIN, DEX_TOKEN, "x")
            .await
            .unwrap();
        assert!(!updated, "admin but no token row → false");
    }

    async fn seed_admin(pool: &PgPool, account_id: &str) {
        sqlx::query(
            "INSERT INTO admin (account_id) VALUES ($1) ON CONFLICT (account_id) DO NOTHING",
        )
        .bind(account_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn verify_admin_on_writer_true_for_admin(pool: PgPool) {
        seed_admin(&pool, ACC_ADMIN).await;
        let ctrl = make_controller(pool);
        assert!(ctrl.verify_admin_on_writer(ACC_ADMIN).await.unwrap());
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn verify_admin_on_writer_false_for_non_admin(pool: PgPool) {
        let ctrl = make_controller(pool);
        assert!(!ctrl.verify_admin_on_writer(ACC_ADMIN).await.unwrap());
    }
}
