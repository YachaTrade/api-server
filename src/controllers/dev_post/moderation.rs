use super::DevPostController;
use crate::result::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PostMutationContext {
    pub post_id: i64,
    pub token_id: String,
    pub changed: bool,
}
#[derive(Debug)]
pub(crate) enum CommitOutcome<T> {
    Committed(T),
    #[allow(dead_code)] // Unreachable now that writes are single-statement CTEs
    // (no separate commit step can fail ambiguously); kept because the enum
    // and downstream service-layer match arms are out of scope for this change.
    Unknown {
        context: T,
        error: String,
    },
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
        let row: (Option<String>, Option<String>, bool) = sqlx::query_as(
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
        .map_err(|error| AppError::InternalError(error.to_string()))?;

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
        let row: (Option<bool>, Option<String>, Option<bool>, Option<bool>, bool) = sqlx::query_as(
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
        .map_err(|error| AppError::InternalError(error.to_string()))?;

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
