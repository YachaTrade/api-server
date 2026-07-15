//! Dev Post service: cache-aside reads (Redis) + invalidation on writes,
//! backed by `DevPostController` for postgres access.
//!
//! Redis is advisory: any cache miss/error falls through to the controller,
//! and cache writes/deletes are best-effort (`let _ = ...`) so a Redis outage
//! never fails a request.

use std::sync::Arc;

use crate::controllers::dev_post::DevPostController;
use crate::controllers::dev_post::moderation::{
    CommitOutcome, ModerationAction, ModerationAudit, ModerationContext, PostMutationContext,
};
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
    #[cfg(test)]
    cache_fill_barriers: Option<CacheFillBarriers>,
}

#[cfg(test)]
#[derive(Clone)]
struct CacheFillHook {
    after_database_read: Arc<tokio::sync::Barrier>,
    allow_cache_set: Arc<tokio::sync::Barrier>,
}
#[cfg(test)]
#[derive(Clone)]
struct CacheFillBarriers {
    feed: CacheFillHook,
    detail: CacheFillHook,
    trending: CacheFillHook,
    ranking: CacheFillHook,
}
#[cfg(test)]
#[derive(Clone, Copy)]
enum CacheFamily {
    Feed,
    Detail,
    Trending,
    Ranking,
}

type UnitInvalidation<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>;
type RankingInvalidation<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<i64>> + Send + 'a>>;
pub(crate) struct InvalidationFutures<'a> {
    pub(crate) global_feed: UnitInvalidation<'a>,
    pub(crate) token_feed: UnitInvalidation<'a>,
    pub(crate) detail: UnitInvalidation<'a>,
    pub(crate) trending: UnitInvalidation<'a>,
    pub(crate) ranking_generation: RankingInvalidation<'a>,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct InvalidationReport {
    pub(crate) failed_families: Vec<&'static str>,
}

impl DevPostService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self {
            postgres,
            redis,
            #[cfg(test)]
            cache_fill_barriers: None,
        }
    }

    #[cfg(test)]
    fn with_cache_fill_barriers(mut self, barriers: CacheFillBarriers) -> Self {
        self.cache_fill_barriers = Some(barriers);
        self
    }

    #[cfg(test)]
    async fn cache_fill_pause(&self, family: CacheFamily) {
        let Some(b) = &self.cache_fill_barriers else {
            return;
        };
        let hook = match family {
            CacheFamily::Feed => &b.feed,
            CacheFamily::Detail => &b.detail,
            CacheFamily::Trending => &b.trending,
            CacheFamily::Ranking => &b.ranking,
        };
        hook.after_database_read.wait().await;
        hook.allow_cache_set.wait().await;
    }

    pub async fn create_post(
        &self,
        author: &str,
        req: &CreateDevPostRequest,
    ) -> Result<i64, AppError> {
        let id = DevPostController::new(self.postgres.clone())
            .create_post(author, req)
            .await?;
        self.invalidate_after_create(&req.token_id).await;
        Ok(id)
    }

    async fn invalidate_after_create(&self, token_id: &str) {
        let _ = tokio::join!(
            self.redis.delete_devpost_feed("global"),
            self.redis.delete_devpost_feed(token_id),
            self.redis.delete_devpost_trending(),
            self.redis.bump_devpost_ranking_generation(),
        );
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
        self.invalidate_after_edit(post_id, &token_id).await;
        Ok(())
    }

    async fn invalidate_after_edit(&self, post_id: i64, token_id: &str) {
        let _ = tokio::join!(
            self.redis.delete_devpost_feed("global"),
            self.redis.delete_devpost_feed(token_id),
            self.redis.delete_devpost_detail(post_id),
            self.redis.delete_devpost_trending(),
            self.redis.bump_devpost_ranking_generation(),
        );
    }

    /// Invalidates the feed (global + token-scoped) and detail caches for the
    /// deleted post — see `edit_post`.
    pub async fn delete_post(&self, post_id: i64, author: &str) -> Result<(), AppError> {
        let outcome = DevPostController::new(self.postgres.clone())
            .delete_post(post_id, author)
            .await?;
        self.finish_author_delete(outcome).await
    }

    pub async fn delete_post_as_admin(&self, post_id: i64, admin: &str) -> Result<(), AppError> {
        if post_id <= 0 {
            return Err(AppError::BadRequest("post_id must be positive".into()));
        }
        let c = DevPostController::new(self.postgres.clone());
        let o = c
            .delete_post_as_admin(post_id, admin, uuid::Uuid::new_v4())
            .await?;
        self.finish_cms_moderation(&c, o).await
    }
    pub async fn restore_post_as_admin(&self, post_id: i64, admin: &str) -> Result<(), AppError> {
        if post_id <= 0 {
            return Err(AppError::BadRequest("post_id must be positive".into()));
        }
        let c = DevPostController::new(self.postgres.clone());
        let o = c
            .restore_post_as_admin(post_id, admin, uuid::Uuid::new_v4())
            .await?;
        self.finish_cms_moderation(&c, o).await
    }

    async fn finish_author_delete(
        &self,
        outcome: CommitOutcome<PostMutationContext>,
    ) -> Result<(), AppError> {
        match outcome {
            CommitOutcome::Committed(c) => {
                if c.changed {
                    self.invalidate_post_public_caches(c.post_id, &c.token_id)
                        .await;
                }
                Ok(())
            }
            CommitOutcome::Unknown { context, error } => {
                self.invalidate_post_public_caches(context.post_id, &context.token_id)
                    .await;
                tracing::warn!(event="dev_post.author_delete.outcome_unknown",post_id=context.post_id,error=%error);
                Err(AppError::InternalError("outcome_unknown".into()))
            }
        }
    }
    async fn finish_cms_moderation(
        &self,
        controller: &DevPostController,
        outcome: CommitOutcome<ModerationContext>,
    ) -> Result<(), AppError> {
        match outcome {
            CommitOutcome::Committed(c) => {
                self.invalidate_post_public_caches(c.post_id, &c.token_id)
                    .await;
                Ok(())
            }
            CommitOutcome::Unknown { context, error } => {
                let l = controller
                    .get_moderation_audit_on_write_pool(context.audit_id)
                    .await;
                self.finish_cms_unknown_after_lookup(context, error, l)
                    .await
            }
        }
    }
    async fn finish_cms_unknown_after_lookup(
        &self,
        context: ModerationContext,
        commit_error: String,
        lookup: Result<Option<ModerationAudit>, AppError>,
    ) -> Result<(), AppError> {
        if let Ok(Some(a)) = lookup {
            if a.matches(&context) {
                self.invalidate_post_public_caches(context.post_id, &context.token_id)
                    .await;
                return Ok(());
            }
        }
        self.invalidate_post_public_caches(context.post_id, &context.token_id)
            .await;
        tracing::warn!(event="cms.dev_post.moderation.outcome_unknown",post_id=context.post_id,error=%commit_error);
        Err(AppError::InternalError("outcome_unknown".into()))
    }
    async fn invalidate_post_public_caches(&self, post_id: i64, token_id: &str) {
        let f = InvalidationFutures {
            global_feed: Box::pin(self.redis.delete_devpost_feed("global")),
            token_feed: Box::pin(self.redis.delete_devpost_feed(token_id)),
            detail: Box::pin(self.redis.delete_devpost_detail(post_id)),
            trending: Box::pin(self.redis.delete_devpost_trending()),
            ranking_generation: Box::pin(self.redis.bump_devpost_ranking_generation()),
        };
        let _ = self.run_invalidation_futures(post_id, token_id, f).await;
    }
    pub(crate) async fn run_invalidation_futures(
        &self,
        post_id: i64,
        token_id: &str,
        f: InvalidationFutures<'_>,
    ) -> InvalidationReport {
        let (a, b, c, d, e) = tokio::join!(
            f.global_feed,
            f.token_feed,
            f.detail,
            f.trending,
            f.ranking_generation
        );
        let mut r = InvalidationReport::default();
        for (n, x) in [
            ("global_feed", a),
            ("token_feed", b),
            ("detail", c),
            ("trending", d),
        ] {
            if let Err(err) = x {
                r.failed_families.push(n);
                tracing::warn!(event="dev_post.cache_invalidation_failed",family=n,post_id,token_id,error=%err)
            }
        }
        if let Err(err) = e {
            r.failed_families.push("ranking_generation");
            tracing::warn!(event="dev_post.cache_invalidation_failed",family="ranking_generation",post_id,token_id,error=%err)
        }
        r
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
                    #[cfg(test)]
                    self.cache_fill_pause(CacheFamily::Feed).await;
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
                #[cfg(test)]
                self.cache_fill_pause(CacheFamily::Detail).await;
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
                #[cfg(test)]
                self.cache_fill_pause(CacheFamily::Trending).await;
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
        let generation = self.redis.get_devpost_ranking_generation().await;
        self.get_ranking_after_generation_read(page, limit, generation)
            .await
    }

    async fn get_ranking_after_generation_read(
        &self,
        page: i64,
        limit: i64,
        generation: anyhow::Result<i64>,
    ) -> Result<(Vec<RankingRow>, i64), AppError> {
        let generation = match generation {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(
                    event = "dev_post.ranking_generation_read_failed",
                    error = %error
                );
                return DevPostController::new(self.postgres.clone())
                    .get_ranking(page, limit)
                    .await;
            }
        };
        if let Ok(cached) = self
            .redis
            .get_devpost_ranking_response_for_generation(generation, page, limit)
            .await
        {
            return Ok((cached.rankings, cached.total_count));
        }
        let (rankings, total_count) = DevPostController::new(self.postgres.clone())
            .get_ranking(page, limit)
            .await?;
        let resp = RankingResponse {
            rankings,
            total_count,
        };
        #[cfg(test)]
        self.cache_fill_pause(CacheFamily::Ranking).await;
        let _ = self
            .redis
            .set_devpost_ranking_response_for_generation(generation, page, limit, &resp)
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

    fn assert_test_redis_namespace() {
        let raw = std::env::var("REDIS_KEY_PREFIX")
            .expect("Redis tests require an explicit REDIS_KEY_PREFIX");
        let trimmed = raw.trim_end_matches(':');
        assert!(
            trimmed.starts_with("test-") && trimmed.len() > 36,
            "refusing to touch Redis outside a test namespace"
        );
        uuid::Uuid::parse_str(&trimmed[trimmed.len() - 36..])
            .expect("REDIS_KEY_PREFIX must end in a per-process UUID");
        assert!(!crate::config::REDIS_KEY_PREFIX.is_empty());
    }

    fn exact_redis_key(suffix: impl AsRef<str>) -> String {
        assert_test_redis_namespace();
        format!(
            "{}{}",
            crate::config::REDIS_KEY_PREFIX.as_str(),
            suffix.as_ref()
        )
    }

    async fn raw_get(key: &str) -> Option<String> {
        let client = redis::Client::open(std::env::var("REDIS_URL").unwrap()).unwrap();
        let mut conn = client.get_multiplexed_async_connection().await.unwrap();
        conn.get(key).await.unwrap()
    }

    async fn raw_delete(key: &str) {
        let client = redis::Client::open(std::env::var("REDIS_URL").unwrap()).unwrap();
        let mut conn = client.get_multiplexed_async_connection().await.unwrap();
        conn.del::<_, ()>(key).await.unwrap();
    }

    async fn seed_exact_keys(keys: &[String]) {
        for key in keys {
            raw_set(key, "seed").await;
        }
        for key in keys {
            assert_eq!(raw_get(key).await.as_deref(), Some("seed"), "{key}");
        }
    }

    async fn assert_exact_keys_absent(keys: &[String]) {
        for key in keys {
            assert!(raw_get(key).await.is_none(), "not deleted: {key}");
        }
    }

    fn feed_keys(scope: &str) -> [String; 3] {
        [
            exact_redis_key(format!("devpost:feed:{scope}")),
            exact_redis_key(format!("devpost:feed:v2:{scope}")),
            exact_redis_key(format!("devpost:feed:v3:{scope}")),
        ]
    }

    fn detail_keys(post_id: i64) -> [String; 2] {
        [
            exact_redis_key(format!("devpost:detail:{post_id}")),
            exact_redis_key(format!("devpost:detail:v2:{post_id}")),
        ]
    }

    fn trending_keys() -> [String; 2] {
        [
            exact_redis_key("devpost:trending"),
            exact_redis_key("devpost:trending:v2"),
        ]
    }

    fn generation_key() -> String {
        exact_redis_key("devpost:ranking:generation")
    }

    fn ranking_v2_key(generation: i64, page: i64, limit: i64) -> String {
        exact_redis_key(format!("devpost:ranking:v2:{generation}:{page}:{limit}"))
    }

    async fn set_generation(value: i64) {
        raw_set(&generation_key(), &value.to_string()).await;
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

    async fn seed_ranked_post(pool: &sqlx::PgPool, token_id: &str) {
        seed_token(pool, token_id, token_id).await;
        sqlx::query(
            "INSERT INTO dev_post (token_id, author, title, body) VALUES ($1, $1, 'Ranked', 'ranked')",
        )
        .bind(token_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn create_text_post(service: &DevPostService, token: &str, body: &str) -> i64 {
        let post_id = service
            .create_post(
                token,
                &CreateDevPostRequest {
                    token_id: token.to_string(),
                    title: "Announcement".into(),
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
    async fn create_uses_full_payload_invalidation_and_bumps_generation(pool: sqlx::PgPool) {
        assert_test_redis_namespace();
        let token = unique_scope();
        seed_token(&pool, &token, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool, redis.clone());
        raw_delete(&generation_key()).await;
        set_generation(41).await;
        let mut keys = Vec::new();
        keys.extend(feed_keys("global"));
        keys.extend(feed_keys(&token));
        keys.extend(trending_keys());
        seed_exact_keys(&keys).await;

        create_text_post(&service, &token, "created").await;

        assert_exact_keys_absent(&keys).await;
        assert_eq!(redis.get_devpost_ranking_generation().await.unwrap(), 42);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn edit_uses_full_payload_invalidation_and_bumps_generation_once(pool: sqlx::PgPool) {
        assert_test_redis_namespace();
        let token = unique_scope();
        seed_token(&pool, &token, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool, redis.clone());
        let post_id = create_text_post(&service, &token, "before").await;
        set_generation(73).await;
        let mut keys = Vec::new();
        keys.extend(feed_keys("global"));
        keys.extend(feed_keys(&token));
        keys.extend(detail_keys(post_id));
        keys.extend(trending_keys());
        seed_exact_keys(&keys).await;

        service
            .edit_post(
                post_id,
                &token,
                &EditDevPostRequest {
                    title: Default::default(),
                    body: Some("after".into()),
                    image_uris: None,
                },
            )
            .await
            .unwrap();

        assert_exact_keys_absent(&keys).await;
        assert_eq!(redis.get_devpost_ranking_generation().await.unwrap(), 74);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn ranking_captures_generation_once_for_get_miss_and_set(pool: sqlx::PgPool) {
        assert_test_redis_namespace();
        let token = unique_scope();
        seed_ranked_post(&pool, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool, redis.clone());
        set_generation(41).await;
        let captured = redis.get_devpost_ranking_generation().await.unwrap();
        assert_eq!(redis.bump_devpost_ranking_generation().await.unwrap(), 42);
        let page = 1;
        let limit = 17;
        let captured_key = ranking_v2_key(captured, page, limit);
        let next_key = ranking_v2_key(42, page, limit);
        raw_delete(&captured_key).await;
        raw_delete(&next_key).await;

        let (rows, total) = service
            .get_ranking_after_generation_read(page, limit, Ok(captured))
            .await
            .unwrap();

        assert_eq!(total, 1);
        assert_eq!(rows[0].token.token_id, token);
        assert!(raw_get(&captured_key).await.is_some());
        assert!(raw_get(&next_key).await.is_none());
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn ranking_generation_error_bypasses_get_and_set(pool: sqlx::PgPool) {
        assert_test_redis_namespace();
        let token = unique_scope();
        seed_ranked_post(&pool, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool, redis);
        let page = 1;
        let limit = 19;
        let v2_key = ranking_v2_key(0, page, limit);
        let legacy_key = exact_redis_key(format!("devpost:ranking:{page}:{limit}"));
        raw_set(&v2_key, "not-json-v2").await;
        raw_set(&legacy_key, "not-json-legacy").await;

        let (rows, total) = service
            .get_ranking_after_generation_read(
                page,
                limit,
                Err(anyhow::anyhow!("generation read failed")),
            )
            .await
            .unwrap();

        assert_eq!(total, 1);
        assert_eq!(rows[0].token.token_id, token);
        assert_eq!(raw_get(&v2_key).await.as_deref(), Some("not-json-v2"));
        assert_eq!(
            raw_get(&legacy_key).await.as_deref(),
            Some("not-json-legacy")
        );
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn ranking_reader_ignores_legacy_key_and_never_sets_it(pool: sqlx::PgPool) {
        assert_test_redis_namespace();
        let token = unique_scope();
        seed_ranked_post(&pool, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool, redis.clone());
        raw_delete(&generation_key()).await;
        let page = 1;
        let limit = 23;
        let legacy_key = exact_redis_key(format!("devpost:ranking:{page}:{limit}"));
        let v2_key = ranking_v2_key(0, page, limit);
        raw_set(&legacy_key, "not-json-legacy").await;
        raw_delete(&v2_key).await;

        let (rows, total) = service.get_ranking(page, limit).await.unwrap();

        assert_eq!(total, 1);
        assert_eq!(rows[0].token.token_id, token);
        assert_eq!(
            raw_get(&legacy_key).await.as_deref(),
            Some("not-json-legacy")
        );
        assert!(raw_get(&v2_key).await.is_some());
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
            title: "Announcement".into(),
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
    async fn corrupt_v3_payload_falls_back_to_postgres(pool: sqlx::PgPool) {
        dotenv::dotenv().ok();
        let token = unique_scope();
        seed_token(&pool, &token, &token).await;
        let redis = Arc::new(RedisDatabase::new().await);
        let service = service(pool, redis.clone());
        create_text_post(&service, &token, "live").await;
        let raw_key = format!(
            "{}devpost:feed:v3:{}",
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
                    title: Default::default(),
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
            title: "Announcement".into(),
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
