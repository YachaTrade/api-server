use std::sync::Arc;

use anyhow::{anyhow, Context, Result};

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tracing::info;
use utoipa::ToSchema;

use crate::{
    db::postgres::PostgresDatabase,
    types::common::{info::AccountInfo, pagination::PaginationParams},
};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct Thread {
    pub thread_id: i32,
    pub token_id: String,
    pub account_info: AccountInfo,
    pub content: String,
    pub created_at: i64,
    pub root_id: Option<i32>,
    pub likes_count: i32,
    pub reply_count: i32,
    pub image_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ThreadsResponse {
    pub thread: Vec<Thread>,
    pub total_count: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow, ToSchema)]
pub struct ThreadLike {
    #[serde(skip_serializing)]
    pub thread_like_id: i32,
    pub thread_id: i32,
    #[serde(skip_serializing)]
    pub token_id: String,
    #[serde(skip_serializing)]
    pub account_id: String,
    #[serde(skip_serializing)]
    pub created_at: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateThreadRequest {
    pub token_id: String,
    pub content: String,
    pub parent_id: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ThreadResponse {
    pub thread: Thread,
}
#[derive(ToSchema)]
pub struct CreateThreadFormData {
    #[schema(example = json!({
        "token_id": "token_address",
        "content": "Your Content",
        "root_id": "Null or root Thread ID"
    }))]
    pub data: String, // JSON string

    #[schema(format = "binary")]
    pub image: Option<Bytes>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ThreadRequest {
    #[schema(example = 1)]
    pub thread_id: i32,
    pub token_id: String,
}
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
        account_id: String,
        content: String,
        root_id: Option<i32>,
        image_uri: Option<String>,
    ) -> Result<Thread> {
        let timestamp = chrono::Utc::now().timestamp();

        let thread = sqlx::query_as!(
            Thread,
            r#"
            WITH inserted_thread AS (
                INSERT INTO thread (token_id, account_id, content, created_at, root_id, image_uri)
                VALUES ($1, $2, $3, $4, $5, $6)
                RETURNING thread_id, token_id, account_id, content, created_at, root_id, 
                    0 as likes_count,
                    COALESCE((SELECT COUNT(*)::int FROM thread WHERE root_id = thread_id), 0) as reply_count,
                    image_uri
            )
            SELECT t.thread_id, t.token_id, 
                   jsonb_build_object(
                       'account_id', a.account_id,
                       'nickname', a.nickname,
                       'image_uri', a.image_uri,
                       'follower_count', 0,
                       'following_count', 0
                   )::jsonb as "account_info!: AccountInfo",
                   t.content, t.created_at, t.root_id,
                   t.likes_count as "likes_count!",
                   t.reply_count as "reply_count!",
                   t.image_uri
            FROM inserted_thread t
            LEFT JOIN account a ON t.account_id = a.account_id
            "#,
            token_id,
            account_id,
            content,
            timestamp,
            root_id,
            image_uri
        )
        .fetch_one(self.db.get_write_pool())
        .await
        .context("Failed to create thread")?;

        Ok(thread)
    }
    pub async fn get_last_thread_id(&self) -> Result<i32> {
        let result = sqlx::query!(
            r#"
            SELECT COALESCE(MAX(thread_id), 0) as last_id 
            FROM thread
            "#
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .context("Failed to get last thread id")?;

        // Option을 처리하는 방법 1: unwrap_or 사용
        Ok(result.last_id.unwrap_or(0))
    }

    pub async fn get_threads_by_token(
        &self,
        token_id: &str,
        pagination: PaginationParams,
    ) -> Result<ThreadsResponse> {
        let offset = (pagination.page - 1) * pagination.limit;
        let thread = sqlx::query_as!(
            Thread,
            r#"
            SELECT t.thread_id, t.token_id, 
                   jsonb_build_object(
                       'account_id', a.account_id,
                       'nickname', a.nickname,
                       'image_uri', a.image_uri,
                       'follower_count', 0,
                       'following_count', 0
                   )::jsonb as "account_info!: AccountInfo",
                   t.content, t.created_at, t.root_id,
                   0 as "likes_count!",
                   COALESCE(COUNT(tr.thread_id)::int, 0) as "reply_count!",
                   t.image_uri
            FROM thread t
            LEFT JOIN account a ON t.account_id = a.account_id
            LEFT JOIN thread tr ON t.thread_id = tr.root_id
            WHERE t.token_id = $1 AND t.root_id IS NULL
            GROUP BY t.thread_id, t.token_id, t.account_id, a.account_id, a.nickname, a.image_uri,
                     t.content, t.created_at, t.root_id, t.image_uri
            ORDER BY t.created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            token_id,
            pagination.limit,
            offset
        )
        .fetch_all(self.db.get_read_pool())
        .await
        .context("Failed to get threads")?;

        let total_count = self.get_threads_count(token_id).await?;

        Ok(ThreadsResponse {
            thread,
            total_count,
        })
    }

    pub async fn get_threads_count(&self, token_id: &str) -> Result<i64> {
        let result = sqlx::query!(
            r#"
            SELECT COALESCE(count, 0) as count
            FROM thread_count
            WHERE token_id = $1
            "#,
            token_id
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .context("Failed to fetch threads count")?
        .count
        .unwrap_or(0);
        Ok(result)
    }

    pub async fn like_thread(
        &self,
        thread_id: i32,
        account_id: &str,
        token_id: &str,
    ) -> Result<Thread> {
        let mut tx = self
            .db
            .get_write_pool()
            .begin()
            .await
            .context("Failed to begin transaction")?;

        sqlx::query!(
            r#"
            INSERT INTO thread_likes (thread_id, account_id, token_id)
            VALUES ($1, $2, $3)
            ON CONFLICT DO NOTHING
            "#,
            thread_id,
            account_id,
            token_id
        )
        .execute(tx.as_mut())
        .await
        .context("Failed to like thread")?;

        let thread = sqlx::query_as!(
            Thread,
            r#"
            SELECT t.thread_id, t.token_id, 
                   jsonb_build_object(
                       'account_id', a.account_id,
                       'nickname', a.nickname,
                       'image_uri', a.image_uri,
                       'follower_count', 0,
                       'following_count', 0
                   )::jsonb as "account_info!: AccountInfo",
                   t.content, t.created_at, t.root_id,
                   COALESCE(COUNT(DISTINCT tl.thread_like_id), 0)::int as "likes_count!",
                   COALESCE(COUNT(DISTINCT tr.thread_id), 0)::int as "reply_count!",
                   t.image_uri
            FROM thread t
            LEFT JOIN account a ON t.account_id = a.account_id
            LEFT JOIN thread_likes tl ON t.thread_id = tl.thread_id
            LEFT JOIN thread tr ON t.thread_id = tr.root_id
            WHERE t.thread_id = $1
            GROUP BY t.thread_id, t.token_id, t.account_id, a.account_id, a.nickname, a.image_uri,
                     t.content, t.created_at, t.root_id, t.image_uri
            "#,
            thread_id
        )
        .fetch_one(tx.as_mut())
        .await
        .context("Failed to fetch updated thread")?;

        tx.commit().await.context("Failed to commit transaction")?;

        Ok(thread)
    }

    pub async fn unlike_thread(
        &self,
        thread_id: i32,
        account_id: &str,
        token_id: &str,
    ) -> Result<Thread> {
        let mut tx = self
            .db
            .get_write_pool()
            .begin()
            .await
            .context("Failed to begin transaction")?;

        sqlx::query!(
            r#"
            DELETE FROM thread_likes
            WHERE thread_id = $1 AND account_id = $2 AND token_id = $3
            "#,
            thread_id,
            account_id,
            token_id
        )
        .execute(tx.as_mut())
        .await
        .context("Failed to unlike thread")?;

        let thread = sqlx::query_as!(
            Thread,
            r#"
            SELECT t.thread_id, t.token_id, 
                   jsonb_build_object(
                       'account_id', a.account_id,
                       'nickname', a.nickname,
                       'image_uri', a.image_uri,
                       'follower_count', 0,
                       'following_count', 0
                   )::jsonb as "account_info!: AccountInfo",
                   t.content, t.created_at, t.root_id,
                   COALESCE(COUNT(DISTINCT tl.thread_like_id), 0)::int as "likes_count!",
                   COALESCE(COUNT(DISTINCT tr.thread_id), 0)::int as "reply_count!",
                   t.image_uri
            FROM thread t
            LEFT JOIN account a ON t.account_id = a.account_id
            LEFT JOIN thread_likes tl ON t.thread_id = tl.thread_id
            LEFT JOIN thread tr ON t.thread_id = tr.root_id
            WHERE t.thread_id = $1
            GROUP BY t.thread_id, t.token_id, t.account_id, a.account_id, a.nickname, a.image_uri,
                     t.content, t.created_at, t.root_id, t.image_uri
            "#,
            thread_id
        )
        .fetch_one(tx.as_mut())
        .await
        .context("Failed to fetch updated thread")?;

        tx.commit().await.context("Failed to commit transaction")?;

        Ok(thread)
    }

    pub async fn get_thread_like_by_token_and_account(
        &self,
        account_id: &str,
        token_id: &str,
    ) -> Result<Vec<i32>> {
        info!("token_id: {}, account_id: {}", token_id, account_id);
        sqlx::query_scalar!(
            "SELECT thread_id FROM thread_likes WHERE token_id = $1 AND account_id = $2",
            token_id,
            account_id
        )
        .fetch_all(self.db.get_read_pool())
        .await
        .map_err(|e| anyhow!("Failed to fetch thread like: {}", e))
    }
}
