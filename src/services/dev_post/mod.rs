//! Dev Post service: cache-aside reads (Redis) + invalidation on writes,
//! backed by `DevPostController` for postgres access.
//!
//! Redis is advisory: any cache miss/error falls through to the controller,
//! and cache writes/deletes are best-effort (`let _ = ...`) so a Redis outage
//! never fails a request.

use std::sync::Arc;

use crate::controllers::dev_post::DevPostController;
use crate::db::postgres::PostgresDatabase;
use crate::db::redis::RedisDatabase;
use crate::result::AppError;
use crate::types::dev_post::{
    CreateDevPostRequest, DevPostResponse, EditDevPostRequest, FeedBase, RankingResponse,
    RankingRow,
};

pub struct DevPostService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl DevPostService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn create_post(
        &self,
        author: &str,
        req: &CreateDevPostRequest,
    ) -> Result<i64, AppError> {
        let id = DevPostController::new(self.postgres.clone())
            .create_post(author, req)
            .await?;
        let _ = self.redis.delete_devpost_feed("global").await;
        let _ = self.redis.delete_devpost_feed(&req.token_id).await;
        Ok(id)
    }

    /// Invalidates the feed (global + token-scoped) and detail caches for the
    /// edited post — the controller's returned `token_id` is consumed here,
    /// not exposed to callers (see `DevPostController::edit_post`).
    pub async fn edit_post(
        &self,
        post_id: i64,
        author: &str,
        req: &EditDevPostRequest,
    ) -> Result<(), AppError> {
        let token_id = DevPostController::new(self.postgres.clone())
            .edit_post(post_id, author, req)
            .await?;
        let _ = self.redis.delete_devpost_feed("global").await;
        let _ = self.redis.delete_devpost_feed(&token_id).await;
        let _ = self.redis.delete_devpost_detail(post_id).await;
        Ok(())
    }

    /// Invalidates the feed (global + token-scoped) and detail caches for the
    /// deleted post — see `edit_post`.
    pub async fn delete_post(&self, post_id: i64, author: &str) -> Result<(), AppError> {
        let token_id = DevPostController::new(self.postgres.clone())
            .delete_post(post_id, author)
            .await?;
        let _ = self.redis.delete_devpost_feed("global").await;
        let _ = self.redis.delete_devpost_feed(&token_id).await;
        let _ = self.redis.delete_devpost_detail(post_id).await;
        Ok(())
    }

    /// Not cached — a like doesn't invalidate the base caches it's counted
    /// in (see module doc for `get_feed`/`get_post`/`get_trending`'s cached
    /// like_count staleness).
    pub async fn like(&self, post_id: i64, account_id: &str) -> Result<i64, AppError> {
        DevPostController::new(self.postgres.clone())
            .like(post_id, account_id)
            .await
    }

    pub async fn unlike(&self, post_id: i64, account_id: &str) -> Result<i64, AppError> {
        DevPostController::new(self.postgres.clone())
            .unlike(post_id, account_id)
            .await
    }

    pub async fn vote(
        &self,
        post_id: i64,
        account_id: &str,
        option_position: i16,
    ) -> Result<(), AppError> {
        DevPostController::new(self.postgres.clone())
            .vote(post_id, account_id, option_position)
            .await
    }

    /// Cache-aside feed page. Only page 1 at the default limit is cached
    /// (viewer-agnostic base, keyed by token scope); every other page/limit
    /// combination bypasses the cache and goes straight to the controller.
    pub async fn get_feed(
        &self,
        token_id: Option<&str>,
        page: i64,
        limit: i64,
        viewer: Option<&str>,
    ) -> Result<(Vec<DevPostResponse>, i64), AppError> {
        let c = DevPostController::new(self.postgres.clone());
        if page == 1 && limit == crate::types::common::pagination::DEFAULT_LIMIT {
            let scope = token_id.unwrap_or("global");
            let mut fb = match self.redis.get_devpost_feed_base(scope).await {
                Ok(b) => b,
                Err(_) => {
                    let (posts, total) = c.get_feed_base(token_id, 1, limit).await?;
                    let b = FeedBase {
                        posts,
                        total_count: total,
                    };
                    let _ = self.redis.set_devpost_feed_base(scope, &b).await;
                    b
                }
            };
            if let Some(v) = viewer {
                c.apply_personalization(&mut fb.posts, v).await?;
            }
            return Ok((fb.posts, fb.total_count));
        }
        c.get_feed(token_id, page, limit, viewer).await
    }

    /// Cache-aside single-post detail (viewer-agnostic base) + personalization overlay.
    pub async fn get_post(
        &self,
        post_id: i64,
        viewer: Option<&str>,
    ) -> Result<DevPostResponse, AppError> {
        let c = DevPostController::new(self.postgres.clone());
        let mut base = match self.redis.get_devpost_detail_base(post_id).await {
            Ok(b) => b,
            Err(_) => {
                let b = c.get_post_base(post_id).await?;
                let _ = self.redis.set_devpost_detail_base(post_id, &b).await;
                b
            }
        };
        if let Some(v) = viewer {
            c.apply_personalization(std::slice::from_mut(&mut base), v)
                .await?;
        }
        Ok(base)
    }

    /// Read-after-write variant for create/edit/vote responses — deliberately
    /// uncached, see `DevPostController::get_post_rw`.
    pub async fn get_post_rw(
        &self,
        post_id: i64,
        viewer: Option<&str>,
    ) -> Result<DevPostResponse, AppError> {
        DevPostController::new(self.postgres.clone())
            .get_post_rw(post_id, viewer)
            .await
    }

    /// Cache-aside trending posts (viewer-agnostic base) + personalization overlay.
    pub async fn get_trending(
        &self,
        viewer: Option<&str>,
    ) -> Result<Vec<DevPostResponse>, AppError> {
        let c = DevPostController::new(self.postgres.clone());
        let mut base = match self.redis.get_devpost_trending_base().await {
            Ok(b) => b,
            Err(_) => {
                let b = c.get_trending_base().await?;
                let _ = self.redis.set_devpost_trending_base(&b).await;
                b
            }
        };
        if let Some(v) = viewer {
            c.apply_personalization(&mut base, v).await?;
        }
        Ok(base)
    }

    /// Cache-aside ranking page. No personalization applies to ranking, so
    /// the full response is cached rather than just a base.
    pub async fn get_ranking(
        &self,
        page: i64,
        limit: i64,
    ) -> Result<(Vec<RankingRow>, i64), AppError> {
        if let Ok(cached) = self.redis.get_devpost_ranking_response(page, limit).await {
            return Ok((cached.rankings, cached.total_count));
        }
        let (rankings, total_count) = DevPostController::new(self.postgres.clone())
            .get_ranking(page, limit)
            .await?;
        let resp = RankingResponse {
            rankings,
            total_count,
        };
        let _ = self
            .redis
            .set_devpost_ranking_response(page, limit, &resp)
            .await;
        Ok((resp.rankings, resp.total_count))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_token(pool: &sqlx::PgPool, token_id: &str, creator: &str) {
        sqlx::query(
            "INSERT INTO token (token_id, name, symbol, image_uri, creator, created_at, transaction_hash, total_supply)
             VALUES ($1,'T','TKN','img',$2,0,'0xhash',0) ON CONFLICT (token_id) DO NOTHING",
        )
        .bind(token_id)
        .bind(creator)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn create_then_get_round_trips_through_service(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let service = DevPostService::new(
            Arc::new(PostgresDatabase {
                write_pool: pool.clone(),
                read_pool: pool,
            }),
            Arc::new(RedisDatabase::new().await),
        );
        let req = CreateDevPostRequest {
            token_id: "0xToken".into(),
            body: Some("hello from service".into()),
            image_uris: None,
            poll: None,
        };
        let id = service.create_post("0xCreator", &req).await.unwrap();
        let post = service.get_post(id, None).await.unwrap();
        assert_eq!(post.id, id.to_string());
        assert_eq!(post.body, "hello from service");
    }
}
