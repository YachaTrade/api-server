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

    pub async fn pin_post(&self, post_id: i64, account_id: &str) -> Result<(), AppError> {
        let token_id = DevPostController::new(self.postgres.clone())
            .pin_post(post_id, account_id)
            .await?;
        let _ = self.redis.delete_devpost_feed(&token_id).await;
        Ok(())
    }

    pub async fn unpin_post(&self, post_id: i64, account_id: &str) -> Result<(), AppError> {
        let token_id = DevPostController::new(self.postgres.clone())
            .unpin_post(post_id, account_id)
            .await?;
        let _ = self.redis.delete_devpost_feed(&token_id).await;
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
    ) -> Result<FeedBase, AppError> {
        let controller = DevPostController::new(self.postgres.clone());
        let mut feed = if page == 1 && limit == crate::types::common::pagination::DEFAULT_LIMIT {
            let scope = token_id.unwrap_or("global");
            match self.redis.get_devpost_feed_base(scope).await {
                Ok(base) => base,
                Err(_) => {
                    let base = controller.get_feed_base(token_id, page, limit).await?;
                    let _ = self.redis.set_devpost_feed_base(scope, &base).await;
                    base
                }
            }
        } else {
            controller.get_feed_base(token_id, page, limit).await?
        };
        if let Some(viewer) = viewer {
            controller
                .apply_feed_personalization(&mut feed, viewer)
                .await?;
        }
        Ok(feed)
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
    use crate::types::dev_post::{CreatePollOptionRequest, CreatePollRequest};
    use redis::AsyncCommands;

    fn unique_scope() -> String {
        let raw = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        crate::utils::valid_account_id(&format!("0x{}", &raw[..40])).unwrap()
    }

    fn service(pool: sqlx::PgPool, redis: Arc<RedisDatabase>) -> DevPostService {
        DevPostService::new(
            Arc::new(PostgresDatabase {
                write_pool: pool.clone(),
                read_pool: pool,
            }),
            redis,
        )
    }

    async fn raw_set(key: &str, value: &str) {
        let client = redis::Client::open(std::env::var("REDIS_URL").unwrap()).unwrap();
        let mut conn = client.get_multiplexed_async_connection().await.unwrap();
        conn.pset_ex::<_, _, ()>(key, value, *crate::config::DEVPOST_FEED_EXPIRATION)
            .await
            .unwrap();
    }

    async fn create_text_post(service: &DevPostService, token: &str, body: &str) -> i64 {
        let post_id = service
            .create_post(
                token,
                &CreateDevPostRequest {
                    token_id: token.to_string(),
                    body: Some(body.to_string()),
                    image_uris: None,
                    poll: None,
                },
            )
            .await
            .unwrap();
        assert!(post_id > 0);
        post_id
    }

    async fn isolate_post_ids(pool: &sqlx::PgPool) {
        let seed = ((uuid::Uuid::new_v4().as_u128() & ((1_u128 << 60) - 1)) as i64).max(1);
        let applied: i64 =
            sqlx::query_scalar("SELECT setval('dev_post_snowflake_seq'::regclass, $1, false)")
                .bind(seed)
                .fetch_one(pool)
                .await
                .unwrap();
        assert_eq!(applied, seed);
    }

    async fn seed_token(pool: &sqlx::PgPool, token_id: &str, creator: &str) {
        isolate_post_ids(pool).await;
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

    #[sqlx::test(migrations = "./migrations-test")]
    async fn token_feed_cache_miss_and_hit_preserve_pin_shape(pool: sqlx::PgPool) {
        dotenv::dotenv().ok();
        let token = unique_scope();
        seed_token(&pool, &token, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool, redis.clone());
        let pin = create_text_post(&service, &token, "pin").await;
        service.pin_post(pin, &token).await.unwrap();

        let miss = service.get_feed(Some(&token), 1, 10, None).await.unwrap();
        assert_eq!(miss.pin.as_ref().unwrap().id, pin.to_string());
        let hit = service.get_feed(Some(&token), 1, 10, None).await.unwrap();
        assert_eq!(
            serde_json::to_value(&miss).unwrap(),
            serde_json::to_value(&hit).unwrap()
        );
        redis.delete_devpost_feed(&token).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn pin_mutations_invalidate_only_token_feed(pool: sqlx::PgPool) {
        dotenv::dotenv().ok();
        let token = unique_scope();
        seed_token(&pool, &token, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool, redis.clone());
        let post = create_text_post(&service, &token, "post").await;
        let empty = FeedBase {
            pin: None,
            posts: Vec::new(),
            total_count: 0,
        };
        let unrelated_scope = unique_scope();
        redis.set_devpost_feed_base(&token, &empty).await.unwrap();
        redis
            .set_devpost_feed_base(&unrelated_scope, &empty)
            .await
            .unwrap();
        let detail = service.get_post(post, None).await.unwrap();
        redis.set_devpost_detail_base(post, &detail).await.unwrap();

        service.pin_post(post, &token).await.unwrap();
        assert!(redis.get_devpost_feed_base(&token).await.is_err());
        assert!(redis.get_devpost_feed_base(&unrelated_scope).await.is_ok());
        assert!(redis.get_devpost_detail_base(post).await.is_ok());

        redis.set_devpost_feed_base(&token, &empty).await.unwrap();
        service.unpin_post(post, &token).await.unwrap();
        assert!(redis.get_devpost_feed_base(&token).await.is_err());
        assert!(redis.get_devpost_feed_base(&unrelated_scope).await.is_ok());

        redis.delete_devpost_feed(&unrelated_scope).await.unwrap();
        redis.delete_devpost_detail(post).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn corrupt_v2_payload_falls_back_to_postgres(pool: sqlx::PgPool) {
        dotenv::dotenv().ok();
        let token = unique_scope();
        seed_token(&pool, &token, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool, redis.clone());
        create_text_post(&service, &token, "live").await;
        let raw_key = format!(
            "{}devpost:feed:v2:{}",
            crate::config::REDIS_KEY_PREFIX.as_str(),
            token
        );
        raw_set(&raw_key, "not-json").await;
        let client = redis::Client::open(std::env::var("REDIS_URL").unwrap()).unwrap();
        let mut conn = client.get_multiplexed_async_connection().await.unwrap();
        let ttl_ms: i64 = conn.pttl(&raw_key).await.unwrap();
        assert!(ttl_ms > 0);

        let feed = service.get_feed(Some(&token), 1, 10, None).await.unwrap();
        assert_eq!(feed.posts.len(), 1);
        assert_eq!(feed.posts[0].body, "live");
        redis.delete_devpost_feed(&token).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn non_default_limit_and_later_pages_bypass_page_one_cache(pool: sqlx::PgPool) {
        dotenv::dotenv().ok();
        let token = unique_scope();
        seed_token(&pool, &token, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool, redis.clone());
        for index in 0..12 {
            create_text_post(&service, &token, &format!("post-{index}")).await;
        }
        let poisoned = FeedBase {
            pin: None,
            posts: Vec::new(),
            total_count: 999,
        };
        redis
            .set_devpost_feed_base(&token, &poisoned)
            .await
            .unwrap();

        let limit_one = service.get_feed(Some(&token), 1, 1, None).await.unwrap();
        assert_eq!(limit_one.posts.len(), 1);
        assert_eq!(limit_one.total_count, 12);
        let page_two = service.get_feed(Some(&token), 2, 10, None).await.unwrap();
        assert_eq!(page_two.posts.len(), 2);
        assert_eq!(page_two.total_count, 12);
        redis.delete_devpost_feed(&token).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn edit_and_delete_refresh_cached_pinned_post_and_existing_scopes(pool: sqlx::PgPool) {
        dotenv::dotenv().ok();
        let token = unique_scope();
        seed_token(&pool, &token, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool.clone(), redis.clone());
        let post = create_text_post(&service, &token, "before").await;
        service.pin_post(post, &token).await.unwrap();
        service.get_feed(Some(&token), 1, 10, None).await.unwrap();
        service.get_post(post, None).await.unwrap();

        service
            .edit_post(
                post,
                &token,
                &EditDevPostRequest {
                    body: Some("after".into()),
                    image_uris: None,
                },
            )
            .await
            .unwrap();
        assert!(redis.get_devpost_feed_base(&token).await.is_err());
        assert!(redis.get_devpost_detail_base(post).await.is_err());

        service.get_feed(Some(&token), 1, 10, None).await.unwrap();
        service.delete_post(post, &token).await.unwrap();
        assert!(redis.get_devpost_feed_base(&token).await.is_err());
        let mappings: i64 =
            sqlx::query_scalar("SELECT count(*) FROM dev_post_pin WHERE post_id = $1")
                .bind(post)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(mappings, 0);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn cached_base_personalizes_pin_and_posts_independently_per_viewer(pool: sqlx::PgPool) {
        dotenv::dotenv().ok();
        let token = unique_scope();
        let viewer_a = unique_scope();
        let viewer_b = unique_scope();
        seed_token(&pool, &token, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool, redis.clone());
        let make_request = |body: &str| CreateDevPostRequest {
            token_id: token.clone(),
            body: Some(body.to_string()),
            image_uris: None,
            poll: Some(CreatePollRequest {
                options: vec![
                    CreatePollOptionRequest {
                        label: "A".into(),
                        image_uri: None,
                    },
                    CreatePollOptionRequest {
                        label: "B".into(),
                        image_uri: None,
                    },
                ],
            }),
        };
        let ordinary = service
            .create_post(&token, &make_request("ordinary"))
            .await
            .unwrap();
        let pin = service
            .create_post(&token, &make_request("pin"))
            .await
            .unwrap();
        service.pin_post(pin, &token).await.unwrap();

        service.like(pin, &viewer_a).await.unwrap();
        service.vote(pin, &viewer_a, 1).await.unwrap();
        service.like(ordinary, &viewer_a).await.unwrap();
        service.vote(ordinary, &viewer_a, 2).await.unwrap();
        service.like(pin, &viewer_b).await.unwrap();
        service.vote(pin, &viewer_b, 2).await.unwrap();

        let anonymous = service.get_feed(Some(&token), 1, 10, None).await.unwrap();
        assert!(!anonymous.pin.as_ref().unwrap().liked_by_me);
        assert_eq!(
            anonymous
                .pin
                .as_ref()
                .unwrap()
                .poll
                .as_ref()
                .unwrap()
                .my_vote_option,
            None
        );
        let viewer_a_feed = service
            .get_feed(Some(&token), 1, 10, Some(&viewer_a))
            .await
            .unwrap();
        assert!(viewer_a_feed.pin.as_ref().unwrap().liked_by_me);
        assert_eq!(
            viewer_a_feed
                .pin
                .as_ref()
                .unwrap()
                .poll
                .as_ref()
                .unwrap()
                .my_vote_option,
            Some(1)
        );
        assert!(viewer_a_feed.posts[0].liked_by_me);
        assert_eq!(
            viewer_a_feed.posts[0].poll.as_ref().unwrap().my_vote_option,
            Some(2)
        );

        let viewer_b_feed = service
            .get_feed(Some(&token), 1, 10, Some(&viewer_b))
            .await
            .unwrap();
        assert!(viewer_b_feed.pin.as_ref().unwrap().liked_by_me);
        assert_eq!(
            viewer_b_feed
                .pin
                .as_ref()
                .unwrap()
                .poll
                .as_ref()
                .unwrap()
                .my_vote_option,
            Some(2)
        );
        assert!(!viewer_b_feed.posts[0].liked_by_me);
        assert_eq!(
            viewer_b_feed.posts[0].poll.as_ref().unwrap().my_vote_option,
            None
        );
        redis.delete_devpost_feed(&token).await.unwrap();
    }
}
