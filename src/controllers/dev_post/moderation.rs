use super::DevPostController;
use crate::result::AppError;

/// Whether a failed write may nonetheless have been applied.
///
/// A single statement is atomic, so an error the server itself reported
/// (constraint, trigger, syntax) means the write definitively did not land —
/// report it as-is. Only transport-level failures (connection dropped, pool
/// timeout) are ambiguous: the server may have committed and the response was
/// lost on the way back. Those are the only ones worth the extra lookup and a
/// defensive cache purge.
fn outcome_is_ambiguous(error: &sqlx::Error) -> bool {
    error.as_database_error().is_none()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PostMutationContext {
    pub post_id: i64,
    pub token_id: String,
    pub changed: bool,
}
#[derive(Debug)]
pub(crate) enum CommitOutcome<T> {
    Committed(T),
    Unknown { context: T, error: String },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModerationAction {
    Delete,
    Restore,
}
impl ModerationAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Delete => "DELETE",
            Self::Restore => "RESTORE",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModerationContext {
    pub audit_id: uuid::Uuid,
    pub admin_account_id: String,
    pub post_id: i64,
    pub token_id: String,
    pub action: ModerationAction,
    pub changed: bool,
}
#[derive(Debug, Clone, sqlx::FromRow)]
pub(crate) struct ModerationAudit {
    pub id: uuid::Uuid,
    pub post_id: i64,
    pub token_id: String,
    pub admin_account_id: String,
    pub action: String,
    pub changed: bool,
}
impl ModerationAudit {
    pub(crate) fn matches(&self, e: &ModerationContext) -> bool {
        self.id == e.audit_id
            && self.post_id == e.post_id
            && self.token_id == e.token_id
            && self.admin_account_id == e.admin_account_id
            && self.action == e.action.as_str()
            && self.changed == e.changed
    }
}

impl DevPostController {
    /// Single-statement CTE: `target` locks the row (`FOR UPDATE`) and carries
    /// the author authorization check; `pin_del`/`soft` both gate on
    /// `EXISTS (SELECT 1 FROM target WHERE target.author = $2)` so a
    /// non-author call touches nothing. `EXISTS (SELECT 1 FROM soft)` reports
    /// whether the row was actually flipped (idempotent re-delete = false).
    pub(crate) async fn delete_post_as_author(
        &self,
        post_id: i64,
        author: &str,
    ) -> Result<CommitOutcome<PostMutationContext>, AppError> {
        let pool = self.db.get_write_pool();
        let row: (Option<String>, Option<String>, bool) = match sqlx::query_as(
            "WITH target AS (
                 SELECT id, token_id, author FROM dev_post WHERE id = $1 AND deleted_at IS NULL FOR UPDATE
             ),
             pin_del AS (
                 DELETE FROM dev_post_pin
                 WHERE post_id = $1 AND EXISTS (SELECT 1 FROM target WHERE target.author = $2)
             ),
             soft AS (
                 UPDATE dev_post SET deleted_at = clock_timestamp()
                 WHERE id = $1 AND deleted_at IS NULL
                   AND EXISTS (SELECT 1 FROM target WHERE target.author = $2)
                 RETURNING token_id
             )
             SELECT (SELECT token_id FROM target) AS token_id,
                    (SELECT author  FROM target) AS author,
                    EXISTS (SELECT 1 FROM soft)  AS changed",
        )
        .bind(post_id)
        .bind(author)
        .fetch_one(pool)
        .await
        {
            Ok(row) => row,
            Err(error) if !outcome_is_ambiguous(&error) => {
                return Err(AppError::InternalError(error.to_string()));
            }
            Err(error) => return Self::recover_author_delete(pool, post_id, error).await,
        };

        let (token_id, existing_author, changed) = row;
        match (token_id, existing_author) {
            (Some(token_id), Some(existing_author)) => {
                if existing_author != author {
                    return Err(AppError::Forbidden("Not the post author".into()));
                }
                Ok(CommitOutcome::Committed(PostMutationContext {
                    post_id,
                    token_id,
                    changed,
                }))
            }
            _ => Err(AppError::NotFound("Post not found".into())),
        }
    }

    /// The statement above may have already committed server-side before the
    /// connection dropped and sqlx surfaced `error`. Look the row up again so
    /// the caller can still purge the public cache — otherwise a soft-deleted
    /// post keeps being served from cache until TTL. Only `deleted_at IS NOT
    /// NULL` is treated as evidence of a real (if ambiguous) commit: a
    /// statement that genuinely failed server-side (e.g. a trigger raising an
    /// exception) rolls back atomically, so the row is found unchanged and
    /// there is nothing to purge — the original error is surfaced instead.
    async fn recover_author_delete(
        pool: &sqlx::PgPool,
        post_id: i64,
        error: sqlx::Error,
    ) -> Result<CommitOutcome<PostMutationContext>, AppError> {
        let looked: Option<(String, bool)> =
            sqlx::query_as("SELECT token_id, deleted_at IS NOT NULL FROM dev_post WHERE id = $1")
                .bind(post_id)
                .fetch_optional(pool)
                .await
                .ok()
                .flatten();
        match looked {
            Some((token_id, changed @ true)) => Ok(CommitOutcome::Unknown {
                context: PostMutationContext {
                    post_id,
                    token_id,
                    changed,
                },
                error: error.to_string(),
            }),
            Some((_, false)) | None => Err(AppError::InternalError(error.to_string())),
        }
    }

    /// Single-statement CTE. Lock order `adm` → `tok` → `locked` mirrors
    /// `pin.rs`'s `token → dev_post` order to avoid deadlocking against
    /// concurrent pin/unpin calls. Independent CTEs run in planner-chosen
    /// order, so the chain is enforced via real data dependencies: `tok`
    /// depends on `EXISTS (SELECT 1 FROM adm)`, and `locked` depends on the
    /// `tok_done` barrier (`SELECT count(*) FROM tok`, always exactly one
    /// row) rather than on `tok` directly, since a plain `EXISTS`/scalar
    /// subquery against `tok` would short-circuit without forcing
    /// evaluation when `tok` is empty (the non-`RESTORE`, missing-token
    /// case). `audit` is gated on `authorized` so the moderation log gets
    /// exactly one row whenever every prior check passed, regardless of
    /// whether `changed` ends up true or false.
    async fn admin_mutate(
        &self,
        post_id: i64,
        admin: &str,
        audit_id: uuid::Uuid,
        action: ModerationAction,
    ) -> Result<CommitOutcome<ModerationContext>, AppError> {
        let pool = self.db.get_write_pool();
        let row: (Option<bool>, Option<String>, Option<bool>, Option<bool>, bool) = match sqlx::query_as(
            "WITH adm AS (
                 SELECT account_id FROM admin WHERE account_id = $2 FOR KEY SHARE
             ),
             post AS (
                 SELECT id, token_id FROM dev_post WHERE id = $1
             ),
             tok AS (
                 SELECT t.token_id FROM token t
                 WHERE t.token_id = (SELECT token_id FROM post) AND EXISTS (SELECT 1 FROM adm)
                 FOR KEY SHARE
             ),
             tok_done AS (SELECT count(*) AS n FROM tok),
             locked AS (
                 SELECT p.deleted_at IS NOT NULL AS was_deleted
                 FROM dev_post p
                 WHERE p.id = $1 AND p.token_id = (SELECT token_id FROM post)
                   AND EXISTS (SELECT 1 FROM adm)
                   AND (SELECT n FROM tok_done) >= 0
                 FOR UPDATE
             ),
             authorized AS (
                 SELECT l.was_deleted FROM locked l
                 WHERE ($4 <> 'RESTORE' OR EXISTS (SELECT 1 FROM tok))
             ),
             pin_del AS (
                 DELETE FROM dev_post_pin
                 WHERE post_id = $1
                   AND EXISTS (SELECT 1 FROM authorized a WHERE $4 = 'DELETE' OR a.was_deleted)
             ),
             mutate AS (
                 UPDATE dev_post
                 SET deleted_at = CASE WHEN $4 = 'DELETE' THEN clock_timestamp() ELSE NULL END
                 WHERE id = $1 AND EXISTS (SELECT 1 FROM authorized)
                   AND (($4 = 'DELETE'  AND deleted_at IS NULL)
                     OR ($4 = 'RESTORE' AND deleted_at IS NOT NULL))
                 RETURNING id
             ),
             audit AS (
                 INSERT INTO dev_post_moderation_log (id, post_id, token_id, admin_account_id, action, changed)
                 SELECT $3, $1, (SELECT token_id FROM post), $2, $4, EXISTS (SELECT 1 FROM mutate)
                 FROM authorized
             )
             SELECT (SELECT true FROM adm)          AS is_admin,
                    (SELECT token_id FROM post)     AS token_id,
                    (SELECT true FROM tok)          AS token_exists,
                    (SELECT true FROM locked)       AS post_locked,
                    EXISTS (SELECT 1 FROM mutate)   AS changed",
        )
        .bind(post_id)
        .bind(admin)
        .bind(audit_id)
        .bind(action.as_str())
        .fetch_one(pool)
        .await
        {
            Ok(row) => row,
            Err(error) if !outcome_is_ambiguous(&error) => {
                return Err(AppError::InternalError(error.to_string()));
            }
            Err(error) => {
                return self
                    .recover_admin_mutate(post_id, admin, audit_id, action, error)
                    .await;
            }
        };

        let (is_admin, token_id, token_exists, post_locked, changed) = row;
        if is_admin != Some(true) {
            return Err(AppError::Forbidden("Not an admin".into()));
        }
        let token_id = token_id.ok_or_else(|| AppError::NotFound("Post not found".into()))?;
        if token_exists != Some(true) && action == ModerationAction::Restore {
            return Err(AppError::Conflict("Token not found".into()));
        }
        if post_locked.is_none() {
            return Err(AppError::NotFound("Post not found".into()));
        }

        Ok(CommitOutcome::Committed(ModerationContext {
            audit_id,
            admin_account_id: admin.into(),
            post_id,
            token_id,
            action,
            changed,
        }))
    }

    /// The audit row is inserted in the same statement as the mutation, so if
    /// it's visible under `audit_id` the statement committed and the row
    /// itself carries the truth of `token_id`/`changed`; if it's confirmed
    /// absent, nothing committed and there's no cache to purge. If the lookup
    /// itself fails we can't tell either way, so fall back to a best-effort
    /// cache purge by `token_id` alone.
    async fn recover_admin_mutate(
        &self,
        post_id: i64,
        admin: &str,
        audit_id: uuid::Uuid,
        action: ModerationAction,
        error: sqlx::Error,
    ) -> Result<CommitOutcome<ModerationContext>, AppError> {
        match self.get_moderation_audit_on_write_pool(audit_id).await {
            Ok(Some(a))
                if a.post_id == post_id
                    && a.admin_account_id == admin
                    && a.action == action.as_str() =>
            {
                Ok(CommitOutcome::Committed(ModerationContext {
                    audit_id,
                    admin_account_id: admin.to_string(),
                    post_id,
                    token_id: a.token_id,
                    action,
                    changed: a.changed,
                }))
            }
            // Audit row confirmed absent = statement did not commit = nothing to purge.
            Ok(_) => Err(AppError::InternalError(error.to_string())),
            // Lookup itself failed = can't tell either way; best-effort purge by token_id.
            Err(_) => {
                let token_id: Option<String> =
                    sqlx::query_scalar("SELECT token_id FROM dev_post WHERE id = $1")
                        .bind(post_id)
                        .fetch_optional(self.db.get_write_pool())
                        .await
                        .ok()
                        .flatten();
                match token_id {
                    Some(token_id) => Ok(CommitOutcome::Unknown {
                        context: ModerationContext {
                            audit_id,
                            admin_account_id: admin.to_string(),
                            post_id,
                            token_id,
                            action,
                            changed: false,
                        },
                        error: error.to_string(),
                    }),
                    None => Err(AppError::InternalError(error.to_string())),
                }
            }
        }
    }

    pub(crate) async fn delete_post_as_admin(
        &self,
        p: i64,
        a: &str,
        id: uuid::Uuid,
    ) -> Result<CommitOutcome<ModerationContext>, AppError> {
        self.admin_mutate(p, a, id, ModerationAction::Delete).await
    }
    pub(crate) async fn restore_post_as_admin(
        &self,
        p: i64,
        a: &str,
        id: uuid::Uuid,
    ) -> Result<CommitOutcome<ModerationContext>, AppError> {
        self.admin_mutate(p, a, id, ModerationAction::Restore).await
    }
    pub(crate) async fn get_moderation_audit_on_write_pool(
        &self,
        id: uuid::Uuid,
    ) -> Result<Option<ModerationAudit>, AppError> {
        sqlx::query_as("SELECT id,post_id,token_id,admin_account_id,action,changed FROM dev_post_moderation_log WHERE id=$1").bind(id).fetch_optional(self.db.get_write_pool()).await.map_err(|e|AppError::InternalError(e.to_string()))
    }
}
