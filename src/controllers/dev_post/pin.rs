use super::DevPostController;
use crate::result::AppError;
use sqlx::{Postgres, Transaction};

pub(super) struct LockedPostContext {
    pub(super) token_id: String,
    pub(super) creator: String,
    pub(super) author: String,
}

fn internal(error: sqlx::Error) -> AppError {
    AppError::InternalError(error.to_string())
}

#[derive(sqlx::FromRow)]
pub(super) struct TokenFeedSelection {
    pub(super) active_pin_id: Option<i64>,
    pub(super) page_ids: Vec<i64>,
    pub(super) total_count: i64,
}

pub(super) async fn select_token_feed(
    pool: &sqlx::PgPool,
    token_id: &str,
    limit: i64,
    offset: i64,
) -> Result<TokenFeedSelection, AppError> {
    sqlx::query_as(
        r#"
        WITH active_pin AS MATERIALIZED (
            SELECT dp.id
            FROM dev_post_pin dpp
            JOIN dev_post dp
              ON dp.id = dpp.post_id
             AND dp.token_id = dpp.token_id
            WHERE dpp.token_id = $1
              AND dp.deleted_at IS NULL
        ),
        ordinary AS (
            SELECT dp.id
            FROM dev_post dp
            WHERE dp.token_id = $1
              AND dp.deleted_at IS NULL
              AND NOT EXISTS (
                  SELECT 1 FROM active_pin ap WHERE ap.id = dp.id
              )
        ),
        page_ids AS (
            SELECT id FROM ordinary ORDER BY id DESC LIMIT $2 OFFSET $3
        )
        SELECT
            (SELECT id FROM active_pin) AS active_pin_id,
            ARRAY(SELECT id FROM page_ids ORDER BY id DESC) AS page_ids,
            (SELECT COUNT(*)::BIGINT FROM ordinary) AS total_count
        "#,
    )
    .bind(token_id)
    .bind(limit)
    .bind(offset)
    .fetch_one(pool)
    .await
    .map_err(internal)
}

pub(super) async fn lock_live_post_context(
    tx: &mut Transaction<'_, Postgres>,
    post_id: i64,
) -> Result<LockedPostContext, AppError> {
    let token_id: Option<String> =
        sqlx::query_scalar("SELECT token_id FROM dev_post WHERE id = $1")
            .bind(post_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(internal)?;
    let token_id = token_id.ok_or_else(|| AppError::NotFound("Post not found".into()))?;

    let creator: Option<String> =
        sqlx::query_scalar("SELECT creator FROM token WHERE token_id = $1 FOR UPDATE")
            .bind(&token_id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(internal)?;
    let creator = creator.ok_or_else(|| AppError::NotFound("Token not found".into()))?;

    let author: Option<String> = sqlx::query_scalar(
        "SELECT author FROM dev_post \
         WHERE id = $1 AND token_id = $2 AND deleted_at IS NULL FOR UPDATE",
    )
    .bind(post_id)
    .bind(&token_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(internal)?;
    let author = author.ok_or_else(|| AppError::NotFound("Post not found".into()))?;

    Ok(LockedPostContext {
        token_id,
        creator,
        author,
    })
}

impl DevPostController {
    pub async fn pin_post(&self, post_id: i64, account_id: &str) -> Result<String, AppError> {
        let mut tx = self.db.get_write_pool().begin().await.map_err(internal)?;
        let context = lock_live_post_context(&mut tx, post_id).await?;
        if context.creator != account_id {
            return Err(AppError::Forbidden(
                "Only the current coin creator can pin posts".into(),
            ));
        }
        if context.author != context.creator {
            return Err(AppError::Forbidden(
                "Only a post authored by the current coin creator can be pinned".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO dev_post_pin (token_id, post_id) VALUES ($1, $2) \
             ON CONFLICT (token_id) DO UPDATE \
             SET post_id = EXCLUDED.post_id, pinned_at = NOW() \
             WHERE dev_post_pin.post_id IS DISTINCT FROM EXCLUDED.post_id",
        )
        .bind(&context.token_id)
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
        tx.commit().await.map_err(internal)?;
        Ok(context.token_id)
    }

    pub async fn unpin_post(&self, post_id: i64, account_id: &str) -> Result<String, AppError> {
        let mut tx = self.db.get_write_pool().begin().await.map_err(internal)?;
        let context = lock_live_post_context(&mut tx, post_id).await?;
        if context.creator != account_id {
            return Err(AppError::Forbidden(
                "Only the current coin creator can unpin posts".into(),
            ));
        }
        sqlx::query("DELETE FROM dev_post_pin WHERE token_id = $1 AND post_id = $2")
            .bind(&context.token_id)
            .bind(post_id)
            .execute(&mut *tx)
            .await
            .map_err(internal)?;
        tx.commit().await.map_err(internal)?;
        Ok(context.token_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::postgres::PostgresDatabase;
    use std::sync::Arc;

    pub(super) const CREATOR: &str = "0x52908400098527886E0F7030069857D2E4169EE7";
    const CREATOR_CASE_MUTATED: &str = "0x52908400098527886e0f7030069857d2e4169ee7";
    pub(super) const NEW_CREATOR: &str = "0xde709f2102306220921060314715629080e2fb77";
    const OTHER: &str = "0x27b1fdb04752bbc536007a920d24acb045561c26";
    pub(super) const TOKEN: &str = "0x0000000000000000000000000000000000007777";

    pub(super) fn controller(pool: sqlx::PgPool) -> DevPostController {
        DevPostController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    pub(super) async fn seed_token(pool: &sqlx::PgPool) {
        sqlx::query(
            "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply) \
             VALUES ($1, 'Token', 'TKN', 'img', $2, 0, '0xhash', 0)",
        )
        .bind(TOKEN)
        .bind(CREATOR)
        .execute(pool)
        .await
        .unwrap();
    }

    pub(super) async fn seed_post(pool: &sqlx::PgPool, author: &str, body: &str) -> i64 {
        sqlx::query_scalar(
            "INSERT INTO dev_post (token_id, author, body) VALUES ($1, $2, $3) RETURNING id",
        )
        .bind(TOKEN)
        .bind(author)
        .bind(body)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn pin_put_is_idempotent_and_replaces_atomically(pool: sqlx::PgPool) {
        seed_token(&pool).await;
        let first = seed_post(&pool, CREATOR, "first").await;
        let second = seed_post(&pool, CREATOR, "second").await;
        let controller = controller(pool.clone());

        assert_eq!(controller.pin_post(first, CREATOR).await.unwrap(), TOKEN);
        sqlx::query("UPDATE dev_post_pin SET pinned_at = TIMESTAMPTZ '2020-01-01 00:00:00Z'")
            .execute(&pool)
            .await
            .unwrap();
        controller.pin_post(first, CREATOR).await.unwrap();
        let same_time: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT pinned_at FROM dev_post_pin WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(same_time.to_rfc3339(), "2020-01-01T00:00:00+00:00");

        controller.pin_post(second, CREATOR).await.unwrap();
        let (post_id, replaced_time): (i64, chrono::DateTime<chrono::Utc>) =
            sqlx::query_as("SELECT post_id, pinned_at FROM dev_post_pin WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(post_id, second);
        assert!(replaced_time > same_time);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn concurrent_puts_leave_exactly_one_mapping(pool: sqlx::PgPool) {
        seed_token(&pool).await;
        let first = seed_post(&pool, CREATOR, "first").await;
        let second = seed_post(&pool, CREATOR, "second").await;
        let first_controller = controller(pool.clone());
        let second_controller = controller(pool.clone());
        let (first_result, second_result) = tokio::join!(
            first_controller.pin_post(first, CREATOR),
            second_controller.pin_post(second, CREATOR)
        );
        first_result.unwrap();
        second_result.unwrap();
        let mappings: i64 =
            sqlx::query_scalar("SELECT count(*) FROM dev_post_pin WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mappings, 1);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn creator_update_serializes_before_pin_authorization(pool: sqlx::PgPool) {
        seed_token(&pool).await;
        let post = seed_post(&pool, CREATOR, "post").await;
        let mut creator_tx = pool.begin().await.unwrap();
        sqlx::query("UPDATE token SET creator = $2 WHERE token_id = $1")
            .bind(TOKEN)
            .bind(NEW_CREATOR)
            .execute(&mut *creator_tx)
            .await
            .unwrap();
        let pin_controller = controller(pool.clone());
        let pin_task = tokio::spawn(async move { pin_controller.pin_post(post, CREATOR).await });
        tokio::task::yield_now().await;
        creator_tx.commit().await.unwrap();
        assert!(matches!(
            pin_task.await.unwrap(),
            Err(AppError::Forbidden(_))
        ));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn pin_authorization_is_exact_and_requires_current_creator_author(pool: sqlx::PgPool) {
        seed_token(&pool).await;
        let creator_post = seed_post(&pool, CREATOR, "creator").await;
        let other_post = seed_post(&pool, OTHER, "other").await;
        let other_token = "0x0000000000000000000000000000000000017777";
        sqlx::query(
            "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply) \
             VALUES ($1, 'Other', 'OTH', 'img', $2, 0, '0xhash2', 0)",
        )
        .bind(other_token)
        .bind(OTHER)
        .execute(&pool)
        .await
        .unwrap();
        let other_token_post: i64 = sqlx::query_scalar(
            "INSERT INTO dev_post (token_id, author, body) VALUES ($1, $2, 'other token') RETURNING id",
        )
        .bind(other_token)
        .bind(OTHER)
        .fetch_one(&pool)
        .await
        .unwrap();
        let controller = controller(pool);

        for result in [
            controller
                .pin_post(creator_post, CREATOR_CASE_MUTATED)
                .await,
            controller.pin_post(creator_post, OTHER).await,
            controller.pin_post(other_post, CREATOR).await,
            controller.pin_post(other_token_post, CREATOR).await,
        ] {
            assert!(matches!(result, Err(AppError::Forbidden(_))));
        }
        controller.pin_post(creator_post, CREATOR).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn unpin_is_idempotent_and_stale_safe(pool: sqlx::PgPool) {
        seed_token(&pool).await;
        let first = seed_post(&pool, CREATOR, "first").await;
        let second = seed_post(&pool, CREATOR, "second").await;
        let controller = controller(pool.clone());

        controller.pin_post(first, CREATOR).await.unwrap();
        controller.pin_post(second, CREATOR).await.unwrap();
        controller.unpin_post(first, CREATOR).await.unwrap();
        controller.unpin_post(first, CREATOR).await.unwrap();
        let current: i64 =
            sqlx::query_scalar("SELECT post_id FROM dev_post_pin WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(current, second);
        controller.unpin_post(second, CREATOR).await.unwrap();
        controller.unpin_post(second, CREATOR).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn transfer_changes_permissions_without_removing_mapping(pool: sqlx::PgPool) {
        seed_token(&pool).await;
        let old_post = seed_post(&pool, CREATOR, "old").await;
        let new_post = seed_post(&pool, NEW_CREATOR, "new").await;
        let controller = controller(pool.clone());
        controller.pin_post(old_post, CREATOR).await.unwrap();
        sqlx::query("UPDATE token SET creator = $2 WHERE token_id = $1")
            .bind(TOKEN)
            .bind(NEW_CREATOR)
            .execute(&pool)
            .await
            .unwrap();

        let retained: i64 =
            sqlx::query_scalar("SELECT post_id FROM dev_post_pin WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(retained, old_post);
        assert!(matches!(
            controller.unpin_post(old_post, CREATOR).await,
            Err(AppError::Forbidden(_))
        ));
        assert!(matches!(
            controller.pin_post(new_post, CREATOR).await,
            Err(AppError::Forbidden(_))
        ));
        assert!(matches!(
            controller.pin_post(old_post, NEW_CREATOR).await,
            Err(AppError::Forbidden(_))
        ));
        controller.pin_post(new_post, NEW_CREATOR).await.unwrap();
        let replaced: i64 =
            sqlx::query_scalar("SELECT post_id FROM dev_post_pin WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(replaced, new_post);

        sqlx::query("UPDATE dev_post_pin SET post_id = $2 WHERE token_id = $1")
            .bind(TOKEN)
            .bind(old_post)
            .execute(&pool)
            .await
            .unwrap();
        controller.unpin_post(old_post, NEW_CREATOR).await.unwrap();
        assert!(matches!(
            controller.pin_post(old_post, NEW_CREATOR).await,
            Err(AppError::Forbidden(_))
        ));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn missing_deleted_or_missing_token_context_is_not_found(pool: sqlx::PgPool) {
        seed_token(&pool).await;
        let deleted = seed_post(&pool, CREATOR, "deleted").await;
        let orphan = seed_post(&pool, CREATOR, "orphan").await;
        sqlx::query("UPDATE dev_post SET deleted_at = NOW() WHERE id = $1")
            .bind(deleted)
            .execute(&pool)
            .await
            .unwrap();
        let controller = controller(pool.clone());
        assert!(matches!(
            controller.pin_post(-1, CREATOR).await,
            Err(AppError::NotFound(_))
        ));
        assert!(matches!(
            controller.pin_post(deleted, CREATOR).await,
            Err(AppError::NotFound(_))
        ));

        sqlx::query("DELETE FROM token WHERE token_id = $1")
            .bind(TOKEN)
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            controller.unpin_post(orphan, CREATOR).await,
            Err(AppError::NotFound(_))
        ));
    }
}
