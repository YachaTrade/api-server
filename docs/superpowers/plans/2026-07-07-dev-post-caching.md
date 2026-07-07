# Dev Post Caching Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Add cache-aside Redis caching to the Dev Post read endpoints (ranking/trending/feed/detail) with per-request personalization overlay, following the house Redis convention.

**Architecture:** Cache the *viewer-agnostic base* of each read; overlay `liked_by_me`/`my_vote_option` per request from the live like/vote tables. TTL-only for ranking/trending; TTL + active invalidation (on create/edit/delete) for feed/detail. Redis is advisory — any miss/error falls through to Postgres.

**Tech Stack:** Rust, sqlx, redis (`pset_ex` JSON), existing `RedisDatabase`/`with_prefix`/`measure_redis!`.

**Spec:** `docs/plans/2026-07-07-dev-post-caching-design.md` (read first).

## Global Constraints
- Branch `feat/dev-post-caching` off `v2` (dev-post already merged). **This runs in a git worktree at `/Users/gyu/project/nads-pump/api-server-caching-wt`** — all work happens there; the crate is `api-server`.
- House Redis idiom: `let key = with_prefix(format!("devpost:...", ...)); conn.pset_ex::<String,String,()>(key, serde_json::to_string(&v)?, *TTL)` wrapped in `measure_redis!("redis.<name>", ...)`. `get_*` returns `Err` on miss. **`delete_*` MUST build the key with the SAME `with_prefix(format!(...))` as the matching `set_*`** (the existing `delete_hype_token_cache` skips the prefix — a latent bug; do NOT copy it).
- Cache is advisory: `get_*` `Err` → recompute; `set_*`/`delete_*` `Err` → log + ignore (never fail a request).
- Personalization (`liked_by_me`, `poll.my_vote_option`) is NEVER stored in the cache — always overlaid per request.
- DB env for tests: `set -a; source .env; set +a`. A local Redis is available (docker `redis:7-alpine`), but tests must not hard-require it — gate Redis-dependent asserts behind availability or keep them to round-trip smoke.
- Stage ONLY your own files per commit; the worktree is clean (isolated), but keep commits scoped.
- End each task green: `cargo test dev_post`, `cargo fmt --check`, `cargo clippy` clean on changed files.

---

## File Structure
- `src/types/dev_post/mod.rs` — add `Deserialize` to cached response types; add `FeedBase`.
- `src/config.rs` — add 4 `DEVPOST_*_EXPIRATION` constants (env-overridable, default 60_000ms).
- `src/controllers/dev_post/mod.rs` — split `hydrate`; add `*_base` variants; `edit_post`/`delete_post` return `token_id`.
- `src/db/redis/mod.rs` — add `get/set/delete_devpost_*` methods.
- `src/services/dev_post/mod.rs` — re-add `redis`; cache-aside + invalidation.
- `src/router/dev_post/handler.rs` — update `DevPostService::new(...)` call sites to pass `state.redis`.

---

## Task 1: Response types — add `Deserialize` + `FeedBase`

**Files:** `src/types/dev_post/mod.rs`.

**Why:** cache `get_*` deserializes JSON → the cached response types need `Deserialize` (they currently derive only `Serialize, ToSchema`).

**Interfaces:**
- Produces: `Deserialize` on `DevPostResponse, TokenSummary, AuthorSummary, PollResponse, PollOptionResponse, RankingRow, RankingResponse`; new `pub struct FeedBase { pub posts: Vec<DevPostResponse>, pub total_count: i64 }` (`Serialize, Deserialize`).

- [ ] **Step 1: Failing test** (serde round-trip)
```rust
#[test]
fn devpost_response_serde_round_trips() {
    let r = DevPostResponse { id: "1".into(), token: TokenSummary{token_id:"0xT".into(),name:"n".into(),symbol:"s".into(),image_uri:None,market_cap:None},
        author: AuthorSummary{account_id:"0xA".into(),nickname:None,image_uri:None}, body:"b".into(), tweet_url:None, images:vec![], poll:None,
        like_count:3, liked_by_me:true, is_edited:false, created_at:chrono::Utc::now(), updated_at:chrono::Utc::now() };
    let j = serde_json::to_string(&r).unwrap();
    let back: DevPostResponse = serde_json::from_str(&j).unwrap();   // fails to compile without Deserialize
    assert_eq!(back.id, "1"); assert_eq!(back.like_count, 3);
}
#[test]
fn feed_base_round_trips() {
    let b = FeedBase { posts: vec![], total_count: 7 };
    let back: FeedBase = serde_json::from_str(&serde_json::to_string(&b).unwrap()).unwrap();
    assert_eq!(back.total_count, 7);
}
```
- [ ] **Step 2: Run → FAIL** (`cargo test dev_post::tests::devpost_response_serde_round_trips` — compile error: no `Deserialize`).
- [ ] **Step 3: Implement** — add `Deserialize` to each `#[derive(...)]` on the 7 types listed; add `FeedBase`:
```rust
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct FeedBase { pub posts: Vec<DevPostResponse>, pub total_count: i64 }
```
(For `chrono::DateTime<Utc>`, `Deserialize` works out of the box with the `serde` feature already used for `Serialize`.)
- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** — `git commit -am "feat(dev-post-cache): add Deserialize + FeedBase for cached responses"`

---

## Task 2: Config TTL constants

**Files:** `src/config.rs`, `.env.example`.

**Interfaces:** Produces `DEVPOST_RANKING_EXPIRATION`, `DEVPOST_TRENDING_EXPIRATION`, `DEVPOST_FEED_EXPIRATION`, `DEVPOST_DETAIL_EXPIRATION` (`u64`, ms).

- [ ] **Step 1:** Add to the `lazy_static!`/`Lazy` block in `config.rs`, **env-overridable with a 60_000 default** (do NOT use the `.expect("... must be set")` form the other constants use — a new hard-required env var would panic every deployment that hasn't set it):
```rust
pub static ref DEVPOST_RANKING_EXPIRATION: u64 = env::var("DEVPOST_RANKING_EXPIRATION")
    .ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(60_000);
// …TRENDING, FEED, DETAIL identical, default 60_000
```
(Match the surrounding declaration style — `lazy_static! { pub static ref … }`.)
- [ ] **Step 2:** Add the 4 vars (commented, `=60000`) to `.env.example` so they're discoverable/tunable.
- [ ] **Step 3:** `cargo build` compiles.
- [ ] **Step 4: Commit** — `git commit -am "feat(dev-post-cache): DEVPOST_* TTL config (60s default, env-overridable)"`

---

## Task 3: Controller — split hydrate, add `*_base`, return `token_id`

**Files:** `src/controllers/dev_post/mod.rs`.

**Interfaces:**
- Produces:
  - `async fn hydrate_base(&self, pool, ids: &[i64]) -> Result<Vec<DevPostResponse>, AppError>` (everything except personalization; `liked_by_me=false`, `poll.my_vote_option=None`).
  - `pub async fn apply_personalization(&self, pool, posts: &mut [DevPostResponse], viewer: &str) -> Result<(), AppError>` (sets `liked_by_me` from `dev_post_like`, `poll.my_vote_option` from `dev_post_poll_vote`, both `WHERE post_id = ANY($1) AND account_id = $2`).
  - `pub async fn get_trending_base(&self) -> Result<Vec<DevPostResponse>, AppError>`, `pub async fn get_feed_base(&self, token_id: Option<&str>, page: i64, limit: i64) -> Result<(Vec<DevPostResponse>, i64), AppError>`, `pub async fn get_post_base(&self, post_id: i64) -> Result<DevPostResponse, AppError>` (viewer-agnostic).
  - `edit_post(...) -> Result<String, AppError>` and `delete_post(...) -> Result<String, AppError>` now return the affected `token_id`.
- Consumes: existing `hydrate`, `get_post_on`, `get_feed`, `get_trending`, `get_post` (refactor to delegate to `*_base` + `apply_personalization`).

- [ ] **Step 1: Failing tests**
```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn hydrate_base_has_no_personalization(pool: sqlx::PgPool) {
    seed_token(&pool, "0xT", "0xC").await;
    let c = ctl(pool.clone());
    let id = c.create_post("0xC", &CreateDevPostRequest{token_id:"0xT".into(),body:Some("b".into()),image_uris:None,
        poll:Some(CreatePollRequest{options:vec![CreatePollOptionRequest{label:"A".into(),image_uri:None},CreatePollOptionRequest{label:"B".into(),image_uri:None}]})}).await.unwrap();
    c.like(id,"0xU").await.unwrap(); c.vote(id,"0xU",2).await.unwrap();
    let base = c.hydrate_base(pool_ref(&c), &[id]).await.unwrap();   // helper to get read pool; or call via get_post_base
    assert!(!base[0].liked_by_me);
    assert_eq!(base[0].poll.as_ref().unwrap().my_vote_option, None);
    assert_eq!(base[0].like_count, 1);                                // counts ARE in base
}
#[sqlx::test(migrations = "./migrations-test")]
async fn apply_personalization_overlays_viewer(pool: sqlx::PgPool) {
    seed_token(&pool, "0xT", "0xC").await;
    let c = ctl(pool.clone());
    let id = c.create_post("0xC", &CreateDevPostRequest{token_id:"0xT".into(),body:None,image_uris:None,
        poll:Some(CreatePollRequest{options:vec![CreatePollOptionRequest{label:"A".into(),image_uri:None},CreatePollOptionRequest{label:"B".into(),image_uri:None}]})}).await.unwrap();
    c.like(id,"0xU").await.unwrap(); c.vote(id,"0xU",2).await.unwrap();
    let mut base = c.get_post_base(id).await.map(|p| vec![p]).unwrap();
    c.apply_personalization(pool_ref(&c), &mut base, "0xU").await.unwrap();
    assert!(base[0].liked_by_me);
    assert_eq!(base[0].poll.as_ref().unwrap().my_vote_option, Some(2));
    // a different viewer sees nothing
    let mut base2 = vec![c.get_post_base(id).await.unwrap()];
    c.apply_personalization(pool_ref(&c), &mut base2, "0xOther").await.unwrap();
    assert!(!base2[0].liked_by_me);
}
#[sqlx::test(migrations = "./migrations-test")]
async fn edit_and_delete_return_token_id(pool: sqlx::PgPool) {
    seed_token(&pool, "0xT", "0xC").await;
    let c = ctl(pool.clone());
    let id = c.create_post("0xC", &CreateDevPostRequest{token_id:"0xT".into(),body:Some("b".into()),image_uris:None,poll:None}).await.unwrap();
    assert_eq!(c.edit_post(id,"0xC",&EditDevPostRequest{body:Some("b2".into()),image_uris:None}).await.unwrap(), "0xT");
    assert_eq!(c.delete_post(id,"0xC").await.unwrap(), "0xT");
}
```
(Provide `pool_ref(&c)` or expose the read pool for the test; simplest is to make `hydrate_base`/`apply_personalization` take `&sqlx::Pool<Postgres>` and pass `c.db.get_read_pool()` — but `db` is private, so add a `#[cfg(test)]` accessor or route the test through `get_post_base` + a public `apply_personalization` that internally uses the read pool. Prefer: `apply_personalization(&self, posts, viewer)` uses `self.db.get_read_pool()` internally so tests don't need a pool handle. Adjust the two `*_base`/personalization signatures accordingly and keep them consistent with the service usage in Task 5.)

> **Design note for the implementer:** the cleanest shape is `apply_personalization(&self, posts: &mut [DevPostResponse], viewer: &str)` (uses `self.db.get_read_pool()` internally) and `hydrate_base(&self, pool, ids)` staying pool-parameterized (so `get_post_rw` can still base-hydrate on the write pool). Reconcile the two and update the tests to match what you build. Existing callers of `edit_post`/`delete_post` (the handler in Task 5) must handle the new `String` return.

- [ ] **Step 2: Run → FAIL.**
- [ ] **Step 3: Implement** — extract the personalization queries out of `hydrate` into `apply_personalization`; make `hydrate_base` produce the non-personalized responses; `hydrate = hydrate_base (+ apply_personalization if viewer)`. Add the three `*_base` methods (delegate: `get_trending`/`get_feed`/`get_post` = `*_base` + personalization). Change `edit_post`/`delete_post` to `SELECT token_id` (or `RETURNING token_id`) and return it. Update any in-crate callers within this file's tests.
- [ ] **Step 4: Run → PASS** (all prior dev_post tests still green — the `edit_post`/`delete_post` return-type change ripples into existing tests; update their `.await.unwrap()` sites).
- [ ] **Step 5: Commit** — `git commit -am "refactor(dev-post): split hydrate into base + personalization; edit/delete return token_id"`

---

## Task 4: Redis cache methods

**Files:** `src/db/redis/mod.rs`.

**Interfaces:** Produces (all on `RedisDatabase`, keys via `with_prefix`):
- `set_devpost_ranking_response(&self, page: i64, limit: i64, resp: &RankingResponse) -> Result<()>` / `get_devpost_ranking_response(&self, page, limit) -> Result<RankingResponse>` — key `devpost:ranking:{page}:{limit}`, TTL `*DEVPOST_RANKING_EXPIRATION`.
- `set_devpost_trending_base(&self, base: &Vec<DevPostResponse>) -> Result<()>` / `get_devpost_trending_base(&self) -> Result<Vec<DevPostResponse>>` — key `devpost:trending`, TTL `*DEVPOST_TRENDING_EXPIRATION`.
- `set_devpost_feed_base(&self, scope: &str, base: &FeedBase) -> Result<()>` / `get_devpost_feed_base(&self, scope: &str) -> Result<FeedBase>` / `delete_devpost_feed(&self, scope: &str) -> Result<()>` — key `devpost:feed:{scope}`, TTL `*DEVPOST_FEED_EXPIRATION`.
- `set_devpost_detail_base(&self, post_id: i64, base: &DevPostResponse) -> Result<()>` / `get_devpost_detail_base(&self, post_id) -> Result<DevPostResponse>` / `delete_devpost_detail(&self, post_id: i64) -> Result<()>` — key `devpost:detail:{post_id}`, TTL `*DEVPOST_DETAIL_EXPIRATION`.

- [ ] **Step 1:** Implement each following the exact `set_token_response`/`get_token_response` idiom (shown below), and `del` for the delete methods with the SAME `with_prefix` key:
```rust
pub async fn set_devpost_feed_base(&self, scope: &str, base: &FeedBase) -> Result<()> {
    let mut conn = self.conn.as_ref().clone();
    let key = with_prefix(format!("devpost:feed:{}", scope));
    let json = serde_json::to_string(base)?;
    measure_redis!("redis.set_devpost_feed_base",
        conn.pset_ex::<String, String, ()>(key, json, *DEVPOST_FEED_EXPIRATION))?;
    Ok(())
}
pub async fn get_devpost_feed_base(&self, scope: &str) -> Result<FeedBase> {
    let mut conn = self.conn.as_ref().clone();
    let key = with_prefix(format!("devpost:feed:{}", scope));
    let json: String = measure_redis!("redis.get_devpost_feed_base", conn.get::<_, String>(key))?;
    Ok(serde_json::from_str(&json)?)
}
pub async fn delete_devpost_feed(&self, scope: &str) -> Result<()> {
    let mut conn = self.conn.as_ref().clone();
    let key = with_prefix(format!("devpost:feed:{}", scope));
    measure_redis!("redis.delete_devpost_feed", conn.del::<String, ()>(key))?;
    Ok(())
}
```
Add the `DEVPOST_*_EXPIRATION` imports to the `use crate::config::{...}` block. Ranking/trending/detail follow the identical shape with their keys/TTLs.
- [ ] **Step 2:** `cargo build` compiles.
- [ ] **Step 3 (optional round-trip test):** If a test Redis is reachable via env (`REDIS_URL`), add a `#[tokio::test]` that constructs `RedisDatabase`, `set_devpost_feed_base("global", &fb)` then `get_devpost_feed_base("global")` equals it, and `delete_devpost_feed("global")` then `get` returns `Err`. If constructing a test `RedisDatabase` isn't straightforward, SKIP with a `// NOTE:` comment (do not hard-require Redis) — the cache-aside behavior is covered by inspection + Task 5.
- [ ] **Step 4: Commit** — `git commit -am "feat(dev-post-cache): redis get/set/delete for ranking/trending/feed/detail"`

---

## Task 5: Service cache-aside + invalidation + handler wiring

**Files:** `src/services/dev_post/mod.rs`, `src/router/dev_post/handler.rs`.

**Interfaces:** `DevPostService { postgres, redis }`, `new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>)`. Read methods become cache-aside; write methods invalidate.

- [ ] **Step 1:** Re-add `redis: Arc<RedisDatabase>`; `new(postgres, redis)`. Implement (all cache errors fall through / are ignored):
```rust
// ranking (no personalization → full cache)
pub async fn get_ranking(&self, page: i64, limit: i64) -> Result<RankingResponse, AppError> {
    if let Ok(cached) = self.redis.get_devpost_ranking_response(page, limit).await { return Ok(cached); }
    let (rankings, total_count) = DevPostController::new(self.postgres.clone()).get_ranking(page, limit).await?;
    let resp = RankingResponse { rankings, total_count };
    let _ = self.redis.set_devpost_ranking_response(page, limit, &resp).await;
    Ok(resp)
}
// trending (base cache + overlay)
pub async fn get_trending(&self, viewer: Option<&str>) -> Result<Vec<DevPostResponse>, AppError> {
    let c = DevPostController::new(self.postgres.clone());
    let mut base = match self.redis.get_devpost_trending_base().await {
        Ok(b) => b,
        Err(_) => { let b = c.get_trending_base().await?; let _ = self.redis.set_devpost_trending_base(&b).await; b }
    };
    if let Some(v) = viewer { c.apply_personalization(&mut base, v).await?; }
    Ok(base)
}
// feed (cache page 1 + default limit only) + overlay
pub async fn get_feed(&self, token_id: Option<&str>, page: i64, limit: i64, viewer: Option<&str>) -> Result<(Vec<DevPostResponse>, i64), AppError> {
    let c = DevPostController::new(self.postgres.clone());
    if page == 1 && limit == crate::types::common::pagination::DEFAULT_LIMIT {
        let scope = token_id.unwrap_or("global");
        let mut fb = match self.redis.get_devpost_feed_base(scope).await {
            Ok(b) => b,
            Err(_) => { let (posts, total) = c.get_feed_base(token_id, 1, limit).await?;
                        let b = FeedBase { posts, total_count: total }; let _ = self.redis.set_devpost_feed_base(scope, &b).await; b }
        };
        if let Some(v) = viewer { c.apply_personalization(&mut fb.posts, v).await?; }
        return Ok((fb.posts, fb.total_count));
    }
    c.get_feed(token_id, page, limit, viewer).await   // uncached
}
// detail (base cache + overlay)
pub async fn get_post(&self, post_id: i64, viewer: Option<&str>) -> Result<DevPostResponse, AppError> {
    let c = DevPostController::new(self.postgres.clone());
    let mut base = match self.redis.get_devpost_detail_base(post_id).await {
        Ok(b) => b,
        Err(_) => { let b = c.get_post_base(post_id).await?; let _ = self.redis.set_devpost_detail_base(post_id, &b).await; b }
    };
    if let Some(v) = viewer { c.apply_personalization(std::slice::from_mut(&mut base), v).await?; }
    Ok(base)
}
// get_post_rw stays uncached (read-after-write): pass-through to controller.get_post_rw
// writes: invalidate
pub async fn create_post(&self, author: &str, req: &CreateDevPostRequest) -> Result<i64, AppError> {
    let id = DevPostController::new(self.postgres.clone()).create_post(author, req).await?;
    let _ = self.redis.delete_devpost_feed("global").await;
    let _ = self.redis.delete_devpost_feed(&req.token_id).await;
    Ok(id)
}
pub async fn edit_post(&self, post_id: i64, author: &str, req: &EditDevPostRequest) -> Result<(), AppError> {
    let token_id = DevPostController::new(self.postgres.clone()).edit_post(post_id, author, req).await?;
    let _ = self.redis.delete_devpost_feed("global").await;
    let _ = self.redis.delete_devpost_feed(&token_id).await;
    let _ = self.redis.delete_devpost_detail(post_id).await;
    Ok(())
}
pub async fn delete_post(&self, post_id: i64, author: &str) -> Result<(), AppError> {
    let token_id = DevPostController::new(self.postgres.clone()).delete_post(post_id, author).await?;
    let _ = self.redis.delete_devpost_feed("global").await;
    let _ = self.redis.delete_devpost_feed(&token_id).await;
    let _ = self.redis.delete_devpost_detail(post_id).await;
    Ok(())
}
// like / unlike / vote: unchanged pass-throughs, NO cache ops
```
Keep the round-trip service test (`create_then_get_round_trips_through_service`) working — construct the test service with a real `RedisDatabase` (from env) OR, if that's awkward in `#[sqlx::test]`, keep the existing test pointed at a service built the same way `HypeService` tests build one. If Redis truly can't be constructed in the test harness, the round-trip test may need the service to accept the same test-Redis the codebase already uses elsewhere — follow that precedent; do not invent a mock.
- [ ] **Step 2:** Update `src/router/dev_post/handler.rs` — every `DevPostService::new(state.postgres.clone())` → `DevPostService::new(state.postgres.clone(), state.redis.clone())` (all handler call sites).
- [ ] **Step 3:** `cargo test dev_post` (existing 30 + adjusted) passes; `cargo build` compiles; `cargo fmt`/`clippy` clean.
- [ ] **Step 4: Commit** — `git commit -am "feat(dev-post-cache): service cache-aside + invalidation; wire redis into handlers"`

---

## Self-Review (plan author)
**Spec coverage:** Deserialize+FeedBase (T1), TTL constants (T2), hydrate split + `*_base` + token_id return (T3), Redis methods (T4), service cache-aside + invalidation + handler wiring (T5). Personalization overlay (T3+T5), fall-through-on-error (T5), page-1+default-limit-only feed cache (T5), no-invalidation on like/vote (T5), get_post_rw uncached (T5).
**Placeholder scan:** the only soft spots are test-harness Redis construction (T4 Step 3 optional / T5 round-trip) — flagged explicitly as "follow existing precedent or skip, don't mock", because the codebase's Redis-in-test story must be discovered by the implementer, not invented here.
**Type consistency:** `hydrate_base`/`apply_personalization`/`get_*_base` names and the `edit_post`/`delete_post` → `Result<String>` change are used identically in T3 (controller) and T5 (service). `FeedBase` from T1 used in T4/T5. `DEVPOST_*_EXPIRATION` from T2 used in T4.
**Executor verification points (not placeholders — confirm against live code):** how the codebase constructs a `RedisDatabase` in tests (if at all); the exact `lazy_static!` vs `once_cell::Lazy` form in config.rs; whether `apply_personalization` should own its pool (recommended) or be pool-parameterized; the `conn.del::<String,()>` vs `::<&str,()>` signature that compiles for the redis crate version in use.
