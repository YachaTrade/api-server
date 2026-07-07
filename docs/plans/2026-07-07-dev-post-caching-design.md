# Dev Post Caching — Design Spec

- **Date**: 2026-07-07
- **Status**: Approved (brainstorming), pending implementation plan
- **Scope**: Add Redis caching to the (already-shipped) Dev Post read endpoints. Backend only.
- **Branch**: `feat/dev-post-caching` off `v2` (dev-post feature already merged to v2 via #175).
- **Depends on**: the Dev Post feature (`docs/plans/2026-07-07-dev-post-api-design.md`), currently uncached (`DevPostService` is a thin pass-through; caching was deferred).

## 1. Overview

Dev Post reads (`ranking`, `trending`, `feed`, `detail`) currently hit Postgres on every request. This adds **cache-aside Redis caching** following the house convention (bespoke per-response Redis methods + per-type TTL constants + JSON `pset_ex`, e.g. `hype`/`search`/`token`). The one wrinkle is **per-viewer personalization** (`liked_by_me`, `poll.my_vote_option`), which is kept out of the cached value and overlaid per-request so one cache entry serves all viewers.

### Policy (per view)

| View | Cache target | TTL | Active invalidation |
|---|---|---|---|
| `ranking` | full `RankingResponse` (no personalization) | 60s | none (TTL only) |
| `trending` | viewer-agnostic **base** (≤3 posts) + per-request overlay | 60s | none (TTL only) |
| `feed` **(page 1 + default limit only)** | base + overlay, per scope (`global` / `token_id`) | 60s (backstop) | **create / edit / delete** → delete that scope's key |
| `detail` | base + overlay, per `post_id` | 60s (backstop) | **edit / delete** → delete that post's key |

**Rationale:** leaderboards (`ranking`/`trending`) tolerate ≤60s lag → TTL only. `feed` must show a newly created/removed post immediately → active invalidation on create/edit/delete (TTL is only a backstop for like-count staleness). `detail` similarly invalidated on edit/delete. `feed` is cached **only for page 1 at the default page size** (`limit == DEFAULT_LIMIT`); deeper pages and non-default page sizes are **not** cached (infinite-scroll tail is lower value, and a single-parameter key keeps invalidation a one-key delete). `like`/`unlike`/`vote` do **not** invalidate anything — they don't change feed/ranking order, and count staleness is bounded by the 60s TTL.

## 2. Personalization split (the core mechanic)

Today `DevPostController::hydrate(pool, ids, viewer)` builds the full response including `liked_by_me` / `my_vote_option`. Split it so the cacheable part is viewer-agnostic:

- `hydrate_base(pool, ids) -> Vec<DevPostResponse>` — everything except personalization: `liked_by_me = false`, `poll.my_vote_option = None`. **This is what gets cached.**
- `apply_personalization(pool, posts: &mut [DevPostResponse], viewer: &str)` — one batched query each: `SELECT post_id FROM dev_post_like WHERE post_id = ANY($1) AND account_id = $2` → set `liked_by_me`; `SELECT post_id, option_position FROM dev_post_poll_vote WHERE post_id = ANY($1) AND account_id = $2` → set `poll.my_vote_option`. Both indexed by the composite PK prefix (`post_id`).
- `hydrate(pool, ids, viewer)` = `hydrate_base` then, if `viewer.is_some()`, `apply_personalization`. Existing callers (`get_feed`/`get_post`/`get_trending`) keep working unchanged.

`ranking` has **no** personalization (`RankingRow` = token + counts), so it caches its full response with no overlay.

The **service** applies the overlay AFTER reading the base from cache: cache stores base → service (if `viewer` is `Some`) calls `controller.apply_personalization(read_pool, &mut base, viewer)`.

## 3. Redis methods (`src/db/redis/mod.rs`, house pattern: JSON `pset_ex`)

Add (mirroring existing `get_/set_/delete_*_response` methods; keys get the `REDIS_KEY_PREFIX`):

- `set_devpost_ranking_response(page: i64, limit: i64, resp: &RankingResponse)` / `get_devpost_ranking_response(page, limit) -> Result<RankingResponse>` — key `devpost:ranking:{page}:{limit}`.
- `set_devpost_trending_base(base: &Vec<DevPostResponse>)` / `get_devpost_trending_base() -> Result<Vec<DevPostResponse>>` — key `devpost:trending`.
- `set_devpost_feed_base(scope: &str, base: &FeedBase)` / `get_devpost_feed_base(scope) -> Result<FeedBase>` / `delete_devpost_feed(scope: &str)` — key `devpost:feed:{scope}` (scope = `"global"` or the checksummed `token_id`). `FeedBase = { posts: Vec<DevPostResponse>, total_count: i64 }`. **Only page 1 at the default limit is ever stored under this key**, so it is a single key per scope and `delete_devpost_feed(scope)` fully invalidates it.
- `set_devpost_detail_base(post_id: i64, base: &DevPostResponse)` / `get_devpost_detail_base(post_id) -> Result<DevPostResponse>` / `delete_devpost_detail(post_id: i64)` — key `devpost:detail:{post_id}`.

`get_*` return `Err` on miss (existing convention — callers treat any `Err` as miss and fall through). `set_*`/`delete_*` failures are logged and ignored (never fail the request).

## 4. Config TTL constants (`src/config.rs`)

Add (milliseconds — the codebase's `pset_ex` = PSETEX takes ms):

```rust
pub static DEVPOST_RANKING_EXPIRATION:  Lazy<u64> = ... 60_000;  // 60s
pub static DEVPOST_TRENDING_EXPIRATION: Lazy<u64> = ... 60_000;
pub static DEVPOST_FEED_EXPIRATION:     Lazy<u64> = ... 60_000;
pub static DEVPOST_DETAIL_EXPIRATION:   Lazy<u64> = ... 60_000;
```
Follow the exact declaration idiom of the existing `*_EXPIRATION` constants (env-overridable if that's the pattern; otherwise literal). One shared 60s value is fine, but keep them as 4 named constants so each view's TTL can be tuned independently later.

## 5. Controller changes (`src/controllers/dev_post/mod.rs`)

- Split `hydrate` → `hydrate_base` + `apply_personalization` (§2); `hydrate` composes them (unchanged behavior).
- Add `get_trending_base(&self) -> Result<Vec<DevPostResponse>>` (trending's ≤3 posts, no personalization) and `get_feed_base(&self, token_id, page, limit) -> Result<(Vec<DevPostResponse>, i64)>` and `get_post_base(&self, post_id) -> Result<DevPostResponse>` — the viewer-agnostic variants used for caching. Existing `get_trending`/`get_feed`/`get_post` can delegate to `*_base` + `apply_personalization` to avoid duplication.
- `apply_personalization` must be `pub` (service calls it on cached bases).
- `edit_post` and `delete_post` **return the affected `token_id`** (`Result<String, AppError>`) so the service can invalidate that token's feed key. (`create_post` already has `req.token_id`.)
- `get_post_rw` (read-after-write for create/edit/vote) stays **uncached** — it must read fresh from the write pool.

## 6. Service changes (`src/services/dev_post/mod.rs`)

Re-add `redis: Arc<RedisDatabase>` to `DevPostService`; `new(postgres, redis)`. (Handlers construct `DevPostService::new(state.postgres.clone(), state.redis.clone())`.)

Cache-aside per view (all reads on the read pool; any Redis error → fall through to the controller):
- `get_ranking(page, limit)`: `get_devpost_ranking_response` → on miss, `controller.get_ranking` → `set_devpost_ranking_response`. No overlay.
- `get_trending(viewer)`: `get_devpost_trending_base` → on miss, `controller.get_trending_base` → `set`. Then if `viewer` is `Some`, `controller.apply_personalization(read_pool, &mut base, viewer)`. Return `TrendingResponse { posts: base }`.
- `get_feed(token_id, page, limit, viewer)`: **only cache when `page == 1 && limit == DEFAULT_LIMIT`**. If so: `scope = token_id.unwrap_or("global")`; `get_devpost_feed_base(scope)` → miss → `controller.get_feed_base(token_id, 1, limit)` → `set`. Then overlay personalization. Otherwise (`page > 1` or non-default `limit`): `controller.get_feed(...)` directly (uncached).
- `get_post(post_id, viewer)`: `get_devpost_detail_base(post_id)` → miss → `controller.get_post_base(post_id)` → `set`. Then overlay. (NotFound propagates; do not cache NotFound.)
- `get_post_rw`: unchanged, uncached (write-pool read-after-write).

Invalidation (write methods):
- `create_post(author, req)`: after controller create → `delete_devpost_feed("global")` + `delete_devpost_feed(&req.token_id)`.
- `edit_post(post_id, author, req)`: capture the returned `token_id` → `delete_devpost_feed("global")` + `delete_devpost_feed(&token_id)` + `delete_devpost_detail(post_id)`.
- `delete_post(post_id, author)`: capture returned `token_id` → same three deletes as edit.
- `like`/`unlike`/`vote`: no cache operations.

All `delete_*` are best-effort (log + ignore errors).

## 7. Consistency & correctness notes
- **Staleness bounds**: ranking/trending ≤60s; feed/detail like-counts ≤60s, but new/edited/deleted posts are immediate (active invalidation). A just-created post: the create *response* is already fresh (read-after-write via `get_post_rw`); the *feed* is invalidated so the next feed load shows it.
- **Personalization is never cached** — always computed per-request from the live like/vote tables, so it's exact even on a cache hit.
- **Cache is advisory**: every `get_*` miss/error falls through to Postgres. Redis being down degrades to the current (uncached) behavior, never an error.
- **Deleted posts**: `hydrate_base`/`get_*_base` already filter `deleted_at IS NULL` (unchanged); a soft-deleted post's detail key is invalidated on delete, and feed keys too.

## 8. Out of scope / follow-ups
- Feed pages > 1 caching (generation-key scheme) — deferred; page-1-only chosen.
- `market_cap` population is a separate deferred item (unrelated to caching).
- Cache warming / metrics on hit rate — not now.

## 9. Testing
- Controller: unit/integration for `hydrate_base` (personalization fields default false/None) and `apply_personalization` (sets liked_by_me / my_vote_option correctly for a viewer).
- Service (`#[sqlx::test]` with a real-or-`None` Redis per the `HypeService` test precedent): cache hit returns same data as miss; personalization overlay differs per viewer on the same cached base; `create/edit/delete` invalidate the right feed/detail keys; `like/vote` do NOT invalidate.
- Redis method round-trips (set→get) for each new method.
- `cargo test dev_post`, `cargo fmt --check`, `cargo clippy` clean.
