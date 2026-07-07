//! Dev Post service: thin pass-through to the controller.
//!
//! Caching (Redis) for `get_trending`/`get_ranking` is intentionally deferred —
//! it's a perf follow-up, not a correctness requirement, and RedisDatabase has
//! no generic get/set<T> cache API to hang it on yet.

use std::sync::Arc;

use crate::controllers::dev_post::DevPostController;
use crate::db::postgres::PostgresDatabase;
use crate::result::AppError;
use crate::types::dev_post::{
    CreateDevPostRequest, DevPostResponse, EditDevPostRequest, RankingRow,
};

pub struct DevPostService {
    postgres: Arc<PostgresDatabase>,
}

impl DevPostService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    pub async fn create_post(
        &self,
        author: &str,
        req: &CreateDevPostRequest,
    ) -> Result<i64, AppError> {
        DevPostController::new(self.postgres.clone())
            .create_post(author, req)
            .await
    }

    pub async fn edit_post(
        &self,
        post_id: i64,
        author: &str,
        req: &EditDevPostRequest,
    ) -> Result<(), AppError> {
        DevPostController::new(self.postgres.clone())
            .edit_post(post_id, author, req)
            .await
    }

    pub async fn delete_post(&self, post_id: i64, author: &str) -> Result<(), AppError> {
        DevPostController::new(self.postgres.clone())
            .delete_post(post_id, author)
            .await
    }

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

    pub async fn get_feed(
        &self,
        token_id: Option<&str>,
        page: i64,
        limit: i64,
        viewer: Option<&str>,
    ) -> Result<(Vec<DevPostResponse>, i64), AppError> {
        DevPostController::new(self.postgres.clone())
            .get_feed(token_id, page, limit, viewer)
            .await
    }

    pub async fn get_post(
        &self,
        post_id: i64,
        viewer: Option<&str>,
    ) -> Result<DevPostResponse, AppError> {
        DevPostController::new(self.postgres.clone())
            .get_post(post_id, viewer)
            .await
    }

    /// Read-after-write variant for create/edit/vote responses — see
    /// `DevPostController::get_post_rw`.
    pub async fn get_post_rw(
        &self,
        post_id: i64,
        viewer: Option<&str>,
    ) -> Result<DevPostResponse, AppError> {
        DevPostController::new(self.postgres.clone())
            .get_post_rw(post_id, viewer)
            .await
    }

    /// Not cached — see module doc.
    pub async fn get_trending(
        &self,
        viewer: Option<&str>,
    ) -> Result<Vec<DevPostResponse>, AppError> {
        DevPostController::new(self.postgres.clone())
            .get_trending(viewer)
            .await
    }

    /// Not cached — see module doc.
    pub async fn get_ranking(
        &self,
        page: i64,
        limit: i64,
    ) -> Result<(Vec<RankingRow>, i64), AppError> {
        DevPostController::new(self.postgres.clone())
            .get_ranking(page, limit)
            .await
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
        let service = DevPostService::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }));
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
