use super::DevPostController;
use crate::result::AppError;
use sqlx::{Postgres, Transaction};

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

pub(crate) fn classify_commit<T>(context: T, result: Result<(), sqlx::Error>) -> CommitOutcome<T> {
    match result {
        Ok(()) => CommitOutcome::Committed(context),
        Err(e) => CommitOutcome::Unknown {
            context,
            error: e.to_string(),
        },
    }
}
async fn finish_transaction<T>(tx: Transaction<'_, Postgres>, context: T) -> CommitOutcome<T> {
    classify_commit(context, tx.commit().await)
}

async fn apply_soft_delete(
    tx: &mut Transaction<'_, Postgres>,
    post_id: i64,
) -> Result<bool, AppError> {
    sqlx::query("DELETE FROM dev_post_pin WHERE post_id=$1")
        .bind(post_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| AppError::InternalError(e.to_string()))?;
    let r = sqlx::query(
        "UPDATE dev_post SET deleted_at=clock_timestamp() WHERE id=$1 AND deleted_at IS NULL",
    )
    .bind(post_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| AppError::InternalError(e.to_string()))?;
    Ok(r.rows_affected() == 1)
}

impl DevPostController {
    pub(crate) async fn delete_post_as_author(
        &self,
        post_id: i64,
        author: &str,
    ) -> Result<CommitOutcome<PostMutationContext>, AppError> {
        let mut tx = self
            .db
            .get_write_pool()
            .begin()
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;
        let pending:Result<PostMutationContext,AppError>=async { let row:Option<(String,String)>=sqlx::query_as("SELECT token_id,author FROM dev_post WHERE id=$1 AND deleted_at IS NULL FOR UPDATE").bind(post_id).fetch_optional(&mut *tx).await.map_err(|e|AppError::InternalError(e.to_string()))?; let (token_id,owner)=row.ok_or_else(||AppError::NotFound("Post not found".into()))?; if owner!=author{return Err(AppError::Forbidden("Not the post author".into()));} let changed=apply_soft_delete(&mut tx,post_id).await?; Ok(PostMutationContext{post_id,token_id,changed}) }.await;
        let context = match pending {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.rollback().await;
                return Err(e);
            }
        };
        Ok(finish_transaction(tx, context).await)
    }
    async fn admin_mutate(
        &self,
        post_id: i64,
        admin: &str,
        audit_id: uuid::Uuid,
        action: ModerationAction,
    ) -> Result<CommitOutcome<ModerationContext>, AppError> {
        let mut tx = self
            .db
            .get_write_pool()
            .begin()
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;
        let pending:Result<ModerationContext,AppError>=async { let ok:Option<String>=sqlx::query_scalar("SELECT account_id FROM admin WHERE account_id=$1 FOR KEY SHARE").bind(admin).fetch_optional(&mut *tx).await.map_err(|e|AppError::InternalError(e.to_string()))?; if ok.is_none(){return Err(AppError::Forbidden("Not an admin".into()));} let token:Option<String>=sqlx::query_scalar("SELECT token_id FROM dev_post WHERE id=$1").bind(post_id).fetch_optional(&mut *tx).await.map_err(|e|AppError::InternalError(e.to_string()))?; let token_id=token.ok_or_else(||AppError::NotFound("Post not found".into()))?; let token_exists:Option<String>=sqlx::query_scalar("SELECT token_id FROM token WHERE token_id=$1 FOR KEY SHARE").bind(&token_id).fetch_optional(&mut *tx).await.map_err(|e|AppError::InternalError(e.to_string()))?; if token_exists.is_none()&&action==ModerationAction::Restore{return Err(AppError::Conflict("Token not found".into()));} let deleted:Option<(bool,)>=sqlx::query_as("SELECT deleted_at IS NOT NULL FROM dev_post WHERE id=$1 AND token_id=$2 FOR UPDATE").bind(post_id).bind(&token_id).fetch_optional(&mut *tx).await.map_err(|e|AppError::InternalError(e.to_string()))?; let was=deleted.ok_or_else(||AppError::NotFound("Post not found".into()))?.0; let changed=match action {ModerationAction::Delete=>apply_soft_delete(&mut tx,post_id).await?,ModerationAction::Restore=>{if was {sqlx::query("DELETE FROM dev_post_pin WHERE post_id=$1").bind(post_id).execute(&mut *tx).await.map_err(|e|AppError::InternalError(e.to_string()))?; sqlx::query("UPDATE dev_post SET deleted_at=NULL WHERE id=$1").bind(post_id).execute(&mut *tx).await.map_err(|e|AppError::InternalError(e.to_string()))?; true}else{false}}}; let c=ModerationContext{audit_id,admin_account_id:admin.into(),post_id,token_id,action,changed}; sqlx::query("INSERT INTO dev_post_moderation_log (id,post_id,token_id,admin_account_id,action,changed) VALUES ($1,$2,$3,$4,$5,$6)").bind(c.audit_id).bind(c.post_id).bind(&c.token_id).bind(&c.admin_account_id).bind(c.action.as_str()).bind(c.changed).execute(&mut *tx).await.map_err(|e|AppError::InternalError(e.to_string()))?; Ok(c) }.await;
        let c = match pending {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.rollback().await;
                return Err(e);
            }
        };
        Ok(finish_transaction(tx, c).await)
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
