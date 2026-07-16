use super::DevPostController;
use crate::result::AppError;

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

impl DevPostController {
    /// Single-statement CTE. Lock order `tok` (token, `FOR UPDATE`) → `live`
    /// (dev_post, `FOR UPDATE`) matches the old `lock_live_post_context`
    /// order and `admin_mutate`'s `tok` → `locked` order, so concurrent
    /// pin/unpin/moderation calls can't deadlock against each other. `live`
    /// depends on the `tok_done` barrier (`SELECT count(*) FROM tok`) to
    /// force `tok` to be evaluated first — a real data dependency, since
    /// independent CTEs otherwise run in planner-chosen order. `ins` only
    /// fires when both `tok.creator = $2` and `live.author = tok.creator`,
    /// so a non-creator or non-self-authored pin touches nothing.
    ///
    /// `creator_update_serializes_before_pin_authorization` (pin.rs tests)
    /// depends on `tok`'s `FOR UPDATE` blocking on a concurrent creator
    /// update and then re-reading the new creator via EPQ — verified to
    /// still hold with the lock inside a CTE.
    pub async fn pin_post(&self, post_id: i64, account_id: &str) -> Result<String, AppError> {
        let pool = self.db.get_write_pool();
        let row: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "WITH post AS (
                 SELECT token_id FROM dev_post WHERE id = $1
             ),
             tok AS (
                 SELECT t.creator FROM token t WHERE t.token_id = (SELECT token_id FROM post) FOR UPDATE
             ),
             tok_done AS (SELECT count(*) AS n FROM tok),
             live AS (
                 SELECT p.author FROM dev_post p
                 WHERE p.id = $1 AND p.token_id = (SELECT token_id FROM post) AND p.deleted_at IS NULL
                   AND (SELECT n FROM tok_done) >= 0
                 FOR UPDATE
             ),
             ins AS (
                 INSERT INTO dev_post_pin (token_id, post_id)
                 SELECT (SELECT token_id FROM post), $1
                 FROM tok, live
                 WHERE tok.creator = $2 AND live.author = tok.creator
                 ON CONFLICT (token_id) DO UPDATE
                   SET post_id = EXCLUDED.post_id, pinned_at = NOW()
                   WHERE dev_post_pin.post_id IS DISTINCT FROM EXCLUDED.post_id
             )
             SELECT (SELECT token_id FROM post) AS token_id,
                    (SELECT creator FROM tok)   AS creator,
                    (SELECT author FROM live)   AS author",
        )
        .bind(post_id)
        .bind(account_id)
        .fetch_one(pool)
        .await
        .map_err(internal)?;

        let (token_id, creator, author) = row;
        let token_id = token_id.ok_or_else(|| AppError::NotFound("Post not found".into()))?;
        let creator = creator.ok_or_else(|| AppError::NotFound("Token not found".into()))?;
        let author = author.ok_or_else(|| AppError::NotFound("Post not found".into()))?;
        if creator != account_id {
            return Err(AppError::Forbidden(
                "Only the current coin creator can pin posts".into(),
            ));
        }
        if author != creator {
            return Err(AppError::Forbidden(
                "Only a post authored by the current coin creator can be pinned".into(),
            ));
        }
        Ok(token_id)
    }

    /// Same lock skeleton as `pin_post`, but `del` only checks the creator
    /// gate (unpin doesn't require author == creator, matching the original
    /// behavior). `live` is still required so unpinning a soft-deleted post
    /// 404s instead of silently deleting the pin row.
    pub async fn unpin_post(&self, post_id: i64, account_id: &str) -> Result<String, AppError> {
        let pool = self.db.get_write_pool();
        let row: (Option<String>, Option<String>, Option<String>) = sqlx::query_as(
            "WITH post AS (
                 SELECT token_id FROM dev_post WHERE id = $1
             ),
             tok AS (
                 SELECT t.creator FROM token t WHERE t.token_id = (SELECT token_id FROM post) FOR UPDATE
             ),
             tok_done AS (SELECT count(*) AS n FROM tok),
             live AS (
                 SELECT p.author FROM dev_post p
                 WHERE p.id = $1 AND p.token_id = (SELECT token_id FROM post) AND p.deleted_at IS NULL
                   AND (SELECT n FROM tok_done) >= 0
                 FOR UPDATE
             ),
             del AS (
                 DELETE FROM dev_post_pin
                 WHERE token_id = (SELECT token_id FROM post) AND post_id = $1
                   AND EXISTS (SELECT 1 FROM tok WHERE tok.creator = $2)
                   AND EXISTS (SELECT 1 FROM live)
             )
             SELECT (SELECT token_id FROM post) AS token_id,
                    (SELECT creator FROM tok)   AS creator,
                    (SELECT author FROM live)   AS author",
        )
        .bind(post_id)
        .bind(account_id)
        .fetch_one(pool)
        .await
        .map_err(internal)?;

        let (token_id, creator, author) = row;
        let token_id = token_id.ok_or_else(|| AppError::NotFound("Post not found".into()))?;
        let creator = creator.ok_or_else(|| AppError::NotFound("Token not found".into()))?;
        let _author = author.ok_or_else(|| AppError::NotFound("Post not found".into()))?;
        if creator != account_id {
            return Err(AppError::Forbidden(
                "Only the current coin creator can unpin posts".into(),
            ));
        }
        Ok(token_id)
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
