# Dev Post API — Design Spec

- **Date**: 2026-07-07
- **Status**: Approved (brainstorming), pending implementation plan
- **Scope**: API backend only (Rust api-server: router + service + controller + types + migrations). No frontend.
- **Integration branch**: `v2` (feat branch `feat/dev-post-api`, PR base `v2`)
- **Source of truth**: Notion "Api 설계용 Dev Post 주요 화면" (화면 4종: 전체 피드 / 코인상세 / 생성 / 관리)

## 1. Overview

"Dev Post" lets a coin's on-chain **creator** publish posts on their own coin. Each post has a text body,
optional images, an optional embedded tweet (parsed from the body), and an optional **poll** (2–3 options,
auto-closing after 14 days). Any **wallet-connected** user can **like** posts (toggle) and **vote** in polls
(changeable until close). The feature surfaces three aggregate views: a global feed, a "Trending" set
(top 3 by likes in the last 7 days), and a per-coin "Top Active Dev Ranking" (cumulative likes).

### Terminology
- **Dev / creator**: the current `token.creator` (EIP-55) of a token. Only the creator may create/edit/delete
  Dev Posts for that token. `token.creator` is mutable (see `set_creator_history`); authorization always uses
  the **current** value.
- **Post**: one `dev_post` row (body + optional images + optional poll), scoped to exactly one `token_id`.
- **Poll**: at most one poll per post; 2–3 options; closes 14 days after creation.

### Non-goals (out of scope)
- Frontend / TypeScript client.
- Tweet content fetching/rendering (backend only returns the raw tweet URL; frontend renders via fxtwitter).
- Notifications, moderation queue, reporting, comments/replies.
- Cursor pagination (house convention is offset/limit; revisit later if the feed needs it).

## 2. Data model (pgactive-compliant)

The database runs under **pgactive active-active replication**. Design rules enforced here:

- **No `SERIAL`/`BIGSERIAL`** (sequences do not replicate → PK collisions). Legacy `SERIAL` tables
  (`0002_token`, `0009_hype`, `0010_raffle`) are **not** a template.
- The only surrogate key needed is `dev_post.id`, generated with the house pgactive pattern
  (`migrations/0018_api_keys.sql`): a dedicated sequence fed through `pgactive.pgactive_snowflake_id_nextval(...)`,
  yielding a globally-unique, timestamp-prefixed `BIGINT`.
- Every other table uses a **natural composite key** (no sequence) → inherently pgactive-safe.
- **Counts are derived** (`COUNT(*)` / aggregation), never stored in incrementable counter columns.
  Rationale: an increment column (`x = x + 1`) loses concurrent increments under last-write-wins across nodes.
  A like row keyed by `(post_id, account_id)` instead converges correctly on INSERT/INSERT conflict (dedup →
  still exactly one like). Trending/ranking already aggregate, so this is consistent; per-post read cost is
  covered by PK indexes + Redis caching of the trending/ranking views.
- **EIP-55 checksum** is canonical for every address column (`VARCHAR(42)`); never `LOWER()`.
- FKs are kept (`ON DELETE CASCADE`) but are enforced node-locally under pgactive; all our FKs stay within one
  post's row-set, so local enforcement is sufficient. `dev_post` is soft-deleted, so cascade only fires on a
  (rare) hard delete.

### Tables

```sql
-- Surrogate id via pgactive snowflake (prod). See §7 for the migrations-test divergence.
CREATE SEQUENCE IF NOT EXISTS dev_post_snowflake_seq;

CREATE TABLE IF NOT EXISTS dev_post (
    id          BIGINT PRIMARY KEY DEFAULT pgactive.pgactive_snowflake_id_nextval('dev_post_snowflake_seq'),
    token_id    VARCHAR(42) NOT NULL REFERENCES token(token_id),
    author      VARCHAR(42) NOT NULL,                 -- creator wallet at time of posting (EIP-55)
    body        TEXT        NOT NULL DEFAULT '',       -- may be empty; see "at least one" rule
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    edited_at   TIMESTAMPTZ,                           -- set on edit; is_edited = edited_at IS NOT NULL
    deleted_at  TIMESTAMPTZ                            -- soft delete; NULL = visible
);
CREATE INDEX IF NOT EXISTS idx_dev_post_token_created ON dev_post (token_id, created_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_dev_post_created       ON dev_post (created_at DESC)            WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_dev_post_author        ON dev_post (author)                     WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS dev_post_image (
    post_id     BIGINT      NOT NULL REFERENCES dev_post(id) ON DELETE CASCADE,
    position    SMALLINT    NOT NULL,                  -- 0-based order, 0..(MAX_IMAGES-1)
    image_uri   TEXT        NOT NULL,                  -- R2 CDN url (https://storage.nadapp.net/devpost/{uuid})
    PRIMARY KEY (post_id, position)
);

CREATE TABLE IF NOT EXISTS dev_post_poll (
    post_id     BIGINT      PRIMARY KEY REFERENCES dev_post(id) ON DELETE CASCADE,
    closes_at   TIMESTAMPTZ NOT NULL                   -- = dev_post.created_at + INTERVAL '14 days'
);

CREATE TABLE IF NOT EXISTS dev_post_poll_option (
    post_id     BIGINT      NOT NULL REFERENCES dev_post_poll(post_id) ON DELETE CASCADE,
    position    SMALLINT    NOT NULL,                  -- 1..3
    label       TEXT        NOT NULL,
    image_uri   TEXT,                                  -- optional per-option image (R2 url)
    PRIMARY KEY (post_id, position)
);

CREATE TABLE IF NOT EXISTS dev_post_like (
    post_id     BIGINT      NOT NULL REFERENCES dev_post(id) ON DELETE CASCADE,
    account_id  VARCHAR(42) NOT NULL,                  -- liker wallet (EIP-55)
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (post_id, account_id)
);
-- Trending window aggregation (likes in last 7 days, grouped by post):
CREATE INDEX IF NOT EXISTS idx_dev_post_like_created ON dev_post_like (created_at, post_id);

CREATE TABLE IF NOT EXISTS dev_post_poll_vote (
    post_id         BIGINT      NOT NULL REFERENCES dev_post_poll(post_id) ON DELETE CASCADE,
    account_id      VARCHAR(42) NOT NULL,              -- voter wallet (EIP-55)
    option_position SMALLINT    NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (post_id, account_id),
    FOREIGN KEY (post_id, option_position) REFERENCES dev_post_poll_option(post_id, position)
);
```

### Constants (validation)
- `MAX_IMAGES = 4` (body images per post).
- Poll options: `2..=3`; each `label` non-empty; `image_uri` optional.
- Poll duration: `14 days` fixed (`closes_at = created_at + 14d`).
- **At-least-one rule**: a post must have a non-empty `body` OR ≥1 image OR a poll; a fully empty post is rejected (400).

## 3. API endpoints

All EVM address inputs (path/query/body) are EIP-55 normalized at handler entry via
`valid_account_id` / `valid_token_id` / `valid_existing_token_id`. Requests validate via `payload.validate()`
→ `AppError::BadRequest`. Responses are `Json<...>` types deriving `Serialize, ToSchema`, registered in the
`utoipa` openapi block.

### Public (no auth layer; optional-auth personalization)
| Method & path | Purpose |
|---|---|
| `GET /dev-post?token_id=&page=&limit=&direction=` | Feed. With `token_id` → that coin's posts (coin-detail / "Learn More"); without → global "All Dev Posts". Newest first. |
| `GET /dev-post/trending` | "Trending Dev Posts": top 3 posts by like count in the **last 7 days** (fixed size 3). |
| `GET /dev-post/ranking?page=&limit=` | "Top Active Dev Ranking": coins ranked by cumulative likes across their (non-deleted) posts. |
| `GET /dev-post/{post_id}` | Single post detail. |

Public GETs read the session cookie **if present** (no 401 when absent) via a new `optional_session_address`
helper, to populate `liked_by_me` and `my_vote_option` per post.

### Protected (auth layer `authenticate_user`; wallet in `Extension<String>`)
| Method & path | Authorization |
|---|---|
| `POST /dev-post/image` | Any authenticated wallet. Multipart single image → R2 → `{ image_uri }`. 5MB, timeout-exempt. Reuses metadata-service magic-byte validation + allowed formats + NSFW (Rekognition). |
| `POST /dev-post` | `session_address == token.creator` (current), checked on **write pool**. |
| `PATCH /dev-post/{post_id}` | Post author only (`dev_post.author == session_address`). |
| `DELETE /dev-post/{post_id}` | Post author only. Soft delete. |
| `POST /dev-post/{post_id}/like` | Any wallet. Idempotent like (INSERT ON CONFLICT DO NOTHING). |
| `DELETE /dev-post/{post_id}/like` | Any wallet. Idempotent unlike (DELETE). |
| `POST /dev-post/{post_id}/vote` | Any wallet. `{ option_position }`. Rejected if poll missing/closed/post deleted. Upsert (change until close). |

### Request bodies
```jsonc
// POST /dev-post
{
  "token_id": "0x…",              // must be a coin whose current creator == caller
  "body": "gm holders …",         // optional text (may embed an x.com/twitter.com link)
  "image_uris": ["https://storage.nadapp.net/devpost/…", …],   // 0..MAX_IMAGES, from POST /dev-post/image
  "poll": {                        // optional
    "options": [
      { "label": "Option A", "image_uri": "https://…" },       // 2..3 options
      { "label": "Option B" }
    ]
  }
}

// PATCH /dev-post/{post_id}  — body & images only; poll is immutable
{ "body": "…", "image_uris": ["…", …] }   // full replacement of images; omit a field to leave unchanged

// POST /dev-post/{post_id}/vote
{ "option_position": 2 }
```

### Response shapes (key types)
```jsonc
// DevPostResponse (used by feed items, detail, trending)
{
  "id": "123456789",                       // BIGINT serialized as string (avoid JS precision loss)
  "token": { "token_id": "0x…", "name": "…", "symbol": "…", "image_uri": "…", "market_cap": "…" },
  "author": { "account_id": "0x…", "nickname": "…", "image_uri": "…" },
  "body": "…",
  "tweet_url": "https://x.com/…",          // first x.com/twitter.com url parsed from body, else null
  "images": ["https://…", …],
  "poll": {                                 // null if none
    "closes_at": "2026-07-21T…Z",
    "is_closed": false,
    "total_votes": 42,
    "options": [
      { "position": 1, "label": "A", "image_uri": null, "vote_count": 30 },
      { "position": 2, "label": "B", "image_uri": null, "vote_count": 12 }
    ],
    "my_vote_option": 1                     // null if not voted / not authenticated
  },
  "like_count": 128,                        // derived COUNT(*)
  "liked_by_me": true,                      // false if not authenticated
  "is_edited": false,
  "created_at": "…", "updated_at": "…"
}

// GET /dev-post            → { "posts": [DevPostResponse, …], "total_count": 240 }
// GET /dev-post/trending   → { "posts": [DevPostResponse, …] }   // ≤3, ordered by recent_like_count desc
// GET /dev-post/ranking    → { "rankings": [RankingRow, …], "total_count": N }
//   RankingRow: { "rank": 1, "token": {token_id,name,symbol,image_uri,market_cap},
//                 "total_likes": 900, "post_count": 12, "last_posted_at": "…" }
// like endpoints           → { "like_count": 129, "liked_by_me": true }
// vote endpoint            → { "total_votes": 43, "options": [{position,label,vote_count}, …], "my_vote_option": 2 }
```

## 4. Behavior rules

- **Poll close = read-time** (`is_closed = closes_at <= now()`). No cron/background job. `POST …/vote` rejects
  (400/409) when `now() >= closes_at` or the post has no poll or is deleted.
- **Like = toggle**, one per `(post_id, account_id)`. `POST …/like` = INSERT ON CONFLICT DO NOTHING;
  `DELETE …/like` = DELETE. Both return the fresh derived `like_count` + `liked_by_me`. Liking a deleted post → 404.
- **Vote = one per `(post_id, account_id)`, changeable until close** via upsert on the vote PK; `option_position`
  must reference an existing option of that poll. Write is a single atomic statement on the **write pool**.
- **Trending**: `SELECT post_id, COUNT(*) FROM dev_post_like l JOIN dev_post p ON p.id = l.post_id
  WHERE l.created_at >= now() - INTERVAL '7 days' AND p.deleted_at IS NULL GROUP BY post_id ORDER BY count DESC LIMIT 3`,
  then hydrate the 3 posts. Redis-cached (short TTL, e.g. 30–60s), invalidated on like/unlike/create/delete.
- **Ranking (per coin)**: `SELECT token_id, COUNT(like) AS total_likes, COUNT(DISTINCT post) AS post_count,
  MAX(created_at) AS last_posted_at FROM dev_post (non-deleted) LEFT JOIN dev_post_like … GROUP BY token_id
  ORDER BY total_likes DESC` with page/limit, joined to token name/symbol/image and **market cap**
  (see §8 dependency). Redis-cached.
- **Soft delete**: every read filters `deleted_at IS NULL`. Deleted posts vanish from feed, detail, trending,
  ranking, and all like/vote aggregation ("모든 곳 공통").
- **Edit**: body + images only; images are a **full replace** of `dev_post_image`. Sets `edited_at = now()`,
  bumps `updated_at`. Poll (`dev_post_poll` / options / `closes_at`) is immutable after creation (vote integrity).
- **Tweet embed**: store `body` verbatim; on read, parse the first `https://(x.com|twitter.com)/…` URL and return
  it as `tweet_url` (nullable). No network fetch.
- **Optional-auth personalization**: feed/detail/trending read the cookie if present to fill `liked_by_me` and
  `poll.my_vote_option`; unauthenticated callers get `false` / `null` and no error.
- **author field**: set to `token.creator` (== caller) at creation; retained even if the coin's creator later
  changes, so historical posts keep their real author.

## 5. Errors
- `400 BadRequest`: validation (empty post, >MAX_IMAGES, poll option count/label, invalid address, closed-poll vote,
  bad `option_position`).
- `401 Unauthorized`: protected route without a valid session.
- `403 Forbidden`: create by non-creator; edit/delete by non-author.
- `404 NotFound`: post id missing/deleted; token id not found.
- `409 Conflict` (or 400): voting on a closed poll (choose one consistently — plan picks 409).

## 6. Module layout (files to add)
- `src/router/dev_post/{mod.rs, path.rs, handler.rs}` — `router()` (public + auth-layered routes), `DevPostPath` enum.
- `src/services/dev_post/mod.rs` — orchestration + Redis caching (trending/ranking) + invalidation.
- `src/controllers/dev_post/mod.rs` — sqlx: reads on `get_read_pool`; creator-check + writes (create/edit/delete/like/vote) on `get_write_pool`.
- `src/types/dev_post/…` — request/response structs (`Serialize/Deserialize/ToSchema`, `.validate()`).
- `src/db/r2/mod.rs` — add `upload_devpost_image_file` (key `devpost/{uuid}`), reusing metadata validation/NSFW.
- Wire-up: `pub mod dev_post;` in `src/router/mod.rs`; `.merge(dev_post::router(...))` in `src/main.rs`; add all
  paths + schemas to the `utoipa` openapi block.
- `optional_session_address` helper (in `src/middleware.rs` or `src/utils`) — resolves cookie→address if valid, else `None`, never 401.

## 7. Migrations (submodule, dual-track)
`migrations/` is a git submodule (PR base `v2`). Add on a submodule feat branch, push, then bump the parent gitlink.

1. **`migrations/0037_dev_post.sql`** (numbered new-install track) — the pgactive version above
   (`pgactive.pgactive_snowflake_id_nextval`).
2. **`migrations/v2_upgrade_new_tables.sql`** — append the same `CREATE TABLE`/sequence/index statements so
   existing v2-upgraded prod DBs get the tables (wrapped consistent with that file's `BEGIN; … COMMIT;`).
3. **`migrations-test/0037_dev_post.sql`** — a **standalone (non-symlink) divergent copy** using plain
   `nextval('dev_post_snowflake_seq')` instead of the pgactive function (stock Postgres has no pgactive),
   mirroring how `migrations-test/0018_api_keys.sql` diverges. All other numbered files are symlinks; this one
   must be a real file.

## 8. Dependencies to confirm during planning
- **Market cap source** for the ranking row: reuse the existing token market data (the `market`/metadata join
  already used by `token` controllers) rather than computing anew. Confirm the exact column/view and its scale/format.
- **`optional_session_address`**: confirm the cookie name env + `redis.get_address_by_session` / `SessionController`
  path can be called without the enforcing middleware.
- **BIGINT serialization**: serialize `dev_post.id` (and any BIGINT) as a **string** in JSON to avoid JS 2^53
  precision loss (snowflake ids exceed it).

## 9. Caching & performance
- Cache `trending` and `ranking` in Redis (short TTL); invalidate on create/delete/like/unlike.
- Feed (`GET /dev-post`) uncached initially; relies on `idx_dev_post_token_created` / `idx_dev_post_created`.
- Per-post `like_count` derived via PK-prefix index on `dev_post_like(post_id, …)`; batch-count posts on a feed
  page in one grouped query rather than N queries.

## 10. Testing
- `#[sqlx::test(migrations = "./migrations-test")]` integration tests per controller (create → read → like toggle →
  vote change → edit → soft-delete visibility), asserting derived counts and pgactive-safe natural keys.
- Authorization tests: non-creator create → 403; non-author edit/delete → 403; closed-poll vote → 409.
- Table-driven validation tests for request structs (empty post, image cap, poll option bounds).
- `cargo test`, `cargo fmt --check`, `cargo clippy` gates before PR.
```
