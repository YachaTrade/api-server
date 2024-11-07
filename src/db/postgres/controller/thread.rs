use std::sync::Arc;

use crate::db::postgres::{model::Thread, PostgresDatabase};
use anyhow::{anyhow, Context, Result};

pub struct ThreadController {
    db: Arc<PostgresDatabase>,
}

impl ThreadController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        ThreadController { db }
    }

    pub async fn create_thread(
        &self,
        token_id: String,
        author_id: String,
        content: String,
        root_id: Option<i32>,
        image_uri: Option<String>,
    ) -> Result<Thread> {
        let mut tx = self
            .db
            .pool
            .begin()
            .await
            .context("Failed to begin transaction")?;

        sqlx::query!(
            r#"
                INSERT INTO token_reply_count (token_id, reply_count)
                VALUES ($1, 1)
                ON CONFLICT (token_id)
                DO UPDATE SET reply_count = token_reply_count.reply_count + 1
                "#,
            token_id
        )
        .execute(tx.as_mut())
        .await
        .context("Failed to update token_reply_count")?;

        let thread = sqlx::query_as!(
            Thread,
            r#"
            INSERT INTO thread (token_id, author_id, content,root_id,image_uri)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
            token_id,
            author_id,
            content,
            root_id,
            image_uri
        )
        .fetch_one(tx.as_mut())
        .await
        .context("Failed to create thread")?;

        if let Some(root_id) = root_id {
            sqlx::query!(
                r#"
                UPDATE thread
                SET reply_count = reply_count + 1
                WHERE thread_id = $1
                "#,
                root_id
            )
            .execute(tx.as_mut())
            .await
            .context("Failed to update root thread replies count")?;
        }

        tx.commit()
            .await
            .context("Failed to Create Thread commit transaction")?;
        Ok(thread)
    }
    pub async fn get_last_thread_id(&self) -> Result<i32> {
        let result = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(thread_id), 0) as last_id 
            FROM thread
            "#
        )
        .fetch_one(&self.db.pool)
        .await
        .context("Failed to get last thread id")?;

        // Option을 처리하는 방법 1: unwrap_or 사용
        Ok(result.last_id.unwrap_or(0))
    }

    pub async fn get_thread(&self, thread_id: i32) -> Result<Thread> {
        sqlx::query_as!(
            Thread,
            "SELECT * FROM thread WHERE thread_id = $1",
            thread_id
        )
        .fetch_one(&self.db.pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch thread: {}", e))
    }

    pub async fn like_thread(&self, thread_id: i32, user_id: &str) -> Result<Thread> {
        let mut transaction = self.db.pool.begin().await?;

        // Try to insert a new like
        let insert_result = sqlx::query!(
            "INSERT INTO thread_likes (thread_id, account_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            thread_id,
            user_id
        )
        .execute(&mut *transaction)
        .await?;
        let affected_rows = insert_result.rows_affected() as i32;
        // Update and return the thread in one query
        let updated_thread = sqlx::query_as!(
            Thread,
            r#"
            UPDATE thread 
            SET likes_count = CASE 
                WHEN $2 > 0 THEN likes_count + 1 
                ELSE likes_count 
            END 
            WHERE thread_id = $1 
            RETURNING *
            "#,
            thread_id,
            affected_rows
        )
        .fetch_one(&mut *transaction)
        .await?;

        transaction.commit().await?;

        Ok(updated_thread)
    }

    pub async fn unlike_thread(&self, thread_id: i32, user_id: &str) -> Result<Thread> {
        let mut tx = self.db.pool.begin().await?;

        let updated_thread = sqlx::query_as!(
            Thread,
            r#"
            WITH deleted_like AS (
                DELETE FROM thread_likes
                WHERE thread_id = $1 AND account_id = $2
                RETURNING thread_id
            )
            UPDATE thread t
            SET likes_count = GREATEST(t.likes_count - 1, 0)
            WHERE t.thread_id = $1
              AND EXISTS (SELECT 1 FROM deleted_like)
            RETURNING *
            "#,
            thread_id,
            user_id
        )
        .fetch_optional(tx.as_mut())
        .await
        .context("Failed to unlike thread")?;

        let thread = match updated_thread {
            Some(thread) => thread,
            None => {
                // If no update occurred, fetch the original thread
                sqlx::query_as!(
                    Thread,
                    "SELECT * FROM thread WHERE thread_id = $1",
                    thread_id
                )
                .fetch_optional(tx.as_mut())
                .await
                .context("Failed to fetch thread")?
                .ok_or_else(|| anyhow::anyhow!("Thread not found"))?
            }
        };

        tx.commit().await.context("Failed to commit transaction")?;

        Ok(thread)
    }
    #[cfg(test)]
    pub async fn get_thread_replies(&self, root_id: i32) -> Result<Vec<Thread>> {
        sqlx::query_as!(Thread, "SELECT * FROM thread WHERE root_id = $1", root_id)
            .fetch_all(&self.db.pool)
            .await
            .map_err(|e| anyhow!("Failed to fetch thread replies: {}", e))
    }
    #[cfg(test)]
    pub async fn get_token_reply_count(&self, token_id: &str) -> Result<i32> {
        let result = sqlx::query!(
            r#"
            SELECT reply_count FROM token_reply_count WHERE token_id = $1
            "#,
            token_id
        )
        .fetch_one(&self.db.pool)
        .await;
        match result {
            Ok(row) => Ok(row.reply_count),
            Err(e) => Err(anyhow!("Failed to fetch coin replies count: {}", e)),
        }
    }
    #[cfg(test)]
    pub async fn delete_token_reply_count(&self, token_id: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM token_reply_count WHERE token_id = $1",
            token_id
        )
        .execute(&self.db.pool)
        .await?;
        Ok(())
    }
}
