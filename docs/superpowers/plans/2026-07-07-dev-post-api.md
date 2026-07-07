# Dev Post API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a coin-creator "Dev Post" feature (posts with images, embedded tweet, and 14-day polls; wallet users like/vote) to the Rust api-server, exposed as REST endpoints backed by Postgres/Redis/R2.

**Architecture:** Follow the existing 4-tier layout — Router → Handler → Service (Redis cache) → Controller (sqlx) → db. Reads use `get_read_pool()`; creator-authorization and all writes use `get_write_pool()`. Counts are derived (`COUNT(*)`), never stored as increment columns (pgactive last-write-wins safety). Only `dev_post.id` is a surrogate key (pgactive snowflake BIGINT); the other six tables use natural composite keys.

**Tech Stack:** Rust, axum 0.7, sqlx 0.8 (Postgres), redis, aws-sdk-s3 (Cloudflare R2), utoipa (OpenAPI). Tests: `#[sqlx::test(migrations = "./migrations-test")]` + table-driven unit tests.

**Design spec:** `docs/plans/2026-07-07-dev-post-api-design.md` (read it before starting).

## Global Constraints

- Integration branch is **`v2`**; this feature branch is `feat/dev-post-api`; PR base = `v2`.
- EVM addresses are **EIP-55 checksum canonical** (`VARCHAR(42)`); never `LOWER()`. Normalize every address input at handler entry with `valid_account_id` / `valid_token_id` / `valid_existing_token_id` (`src/utils/mod.rs`).
- **No `SERIAL`/`BIGSERIAL`.** Surrogate keys use `pgactive.pgactive_snowflake_id_nextval('<seq>')` (prod) / `nextval('<seq>')` (migrations-test). Everything else is a natural composite key.
- **`dev_post.id` is a BIGINT** — serialize it as a **string** in all JSON responses (JS 2^53 precision).
- `migrations/` is a git **submodule**: commit migrations on a submodule feat branch + push, then bump the parent gitlink. PR base for the submodule is also `v2`.
- Constants: `MAX_IMAGES = 4`, poll options `2..=3`, poll duration `14 days`.
- Error mapping (`src/result.rs`): `AppError::BadRequest`→400, `Unauthorized`→401, `Forbidden`→403, `NotFound`→404, `Conflict`→409, `InternalError`→500.
- Every task ends green on `cargo test`, `cargo fmt --check`, `cargo clippy` (changed crates). Commit per task.

---

## File Structure

**Migrations (submodule `migrations/`):**
- Create `migrations/0037_dev_post.sql` — prod schema (pgactive snowflake).
- Modify `migrations/v2_upgrade_new_tables.sql` — append same tables for v2-upgraded prod DBs.
- Create `migrations-test/0037_dev_post.sql` — standalone divergent copy (`nextval`, no pgactive). **Real file, not a symlink.**

**Types (`src/types/dev_post/`):**
- Create `src/types/dev_post/mod.rs` — request structs (+`validate()`), response structs, constants. One file; it is cohesive and small.

**Controller (`src/controllers/dev_post/`):**
- Create `src/controllers/dev_post/mod.rs` — `DevPostController` (sqlx). Row structs declared inline.

**Service (`src/services/dev_post/`):**
- Create `src/services/dev_post/mod.rs` — `DevPostService` (orchestration + Redis cache for trending/ranking + invalidation).

**R2 (`src/db/r2/mod.rs`):**
- Modify — add `upload_devpost_image_file` (key `devpost/{uuid}`).

**Router (`src/router/dev_post/`):**
- Create `src/router/dev_post/mod.rs` — `router()` (public + auth-layered routes).
- Create `src/router/dev_post/path.rs` — `DevPostPath` enum (`as_str`/`docs_str`).
- Create `src/router/dev_post/handler.rs` — thin handlers.

**Shared helper:**
- Modify `src/middleware.rs` — add `optional_session_address` (cookie→address or `None`, never 401).

**Wiring:**
- Modify `src/router/mod.rs` — `pub mod dev_post;`.
- Modify `src/main.rs` — `.merge(dev_post::router(app_state.clone()))` + utoipa `paths(...)`/`components(schemas(...))`.

**Docs:**
- Create `docs/dev-post-api.md` — API reference (house convention, update before PR).

Registration order in `DevPostPath`/router: static routes (`/dev-post/trending`, `/dev-post/ranking`, `/dev-post/image`) must be declared **before** the `/dev-post/{post_id}` capture so matchit resolves statics first.

---

## Task 1: Migrations (schema)

**Files:**
- Create: `migrations/0037_dev_post.sql`
- Modify: `migrations/v2_upgrade_new_tables.sql`
- Create: `migrations-test/0037_dev_post.sql` (real file, not symlink)

**Interfaces:**
- Produces: tables `dev_post`, `dev_post_image`, `dev_post_poll`, `dev_post_poll_option`, `dev_post_like`, `dev_post_poll_vote`; sequence `dev_post_snowflake_seq`.

- [ ] **Step 1: Write `migrations/0037_dev_post.sql`** (prod, pgactive)

```sql
-- Dev Post feature: coin-creator posts with images, polls, likes, votes.
-- Design: docs/plans/2026-07-07-dev-post-api-design.md
-- pgactive: surrogate key via snowflake; all other tables use natural composite keys.
CREATE SEQUENCE IF NOT EXISTS dev_post_snowflake_seq;

CREATE TABLE IF NOT EXISTS dev_post (
    id          BIGINT PRIMARY KEY DEFAULT pgactive.pgactive_snowflake_id_nextval('dev_post_snowflake_seq'),
    token_id    VARCHAR(42) NOT NULL,
    author      VARCHAR(42) NOT NULL,
    body        TEXT        NOT NULL DEFAULT '',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    edited_at   TIMESTAMPTZ,
    deleted_at  TIMESTAMPTZ
);
CREATE INDEX IF NOT EXISTS idx_dev_post_token_created ON dev_post (token_id, created_at DESC) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_dev_post_created       ON dev_post (created_at DESC)            WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_dev_post_author        ON dev_post (author)                     WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS dev_post_image (
    post_id   BIGINT   NOT NULL REFERENCES dev_post(id) ON DELETE CASCADE,
    position  SMALLINT NOT NULL,
    image_uri TEXT     NOT NULL,
    PRIMARY KEY (post_id, position)
);

CREATE TABLE IF NOT EXISTS dev_post_poll (
    post_id   BIGINT      PRIMARY KEY REFERENCES dev_post(id) ON DELETE CASCADE,
    closes_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS dev_post_poll_option (
    post_id   BIGINT   NOT NULL REFERENCES dev_post_poll(post_id) ON DELETE CASCADE,
    position  SMALLINT NOT NULL,
    label     TEXT     NOT NULL,
    image_uri TEXT,
    PRIMARY KEY (post_id, position)
);

CREATE TABLE IF NOT EXISTS dev_post_like (
    post_id    BIGINT      NOT NULL REFERENCES dev_post(id) ON DELETE CASCADE,
    account_id VARCHAR(42) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (post_id, account_id)
);
CREATE INDEX IF NOT EXISTS idx_dev_post_like_created ON dev_post_like (created_at, post_id);

CREATE TABLE IF NOT EXISTS dev_post_poll_vote (
    post_id         BIGINT      NOT NULL REFERENCES dev_post_poll(post_id) ON DELETE CASCADE,
    account_id      VARCHAR(42) NOT NULL,
    option_position SMALLINT    NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (post_id, account_id),
    FOREIGN KEY (post_id, option_position) REFERENCES dev_post_poll_option(post_id, position)
);
```

- [ ] **Step 2: Append the same statements to `migrations/v2_upgrade_new_tables.sql`**

Open the file, and inside its existing `BEGIN; … COMMIT;` block append the `CREATE SEQUENCE` + all six `CREATE TABLE` + index statements verbatim from Step 1 (they are idempotent `IF NOT EXISTS`). Match the file's existing indentation/comment style.

- [ ] **Step 3: Write `migrations-test/0037_dev_post.sql`** (stock Postgres divergent copy)

Identical to Step 1 **except** the `dev_post.id` default:
```sql
-- migrations-test standalone copy (NOT a symlink): stock Postgres has no pgactive extension,
-- so use plain nextval(). Production uses pgactive.pgactive_snowflake_id_nextval() via migrations/0037.
CREATE SEQUENCE IF NOT EXISTS dev_post_snowflake_seq;
CREATE TABLE IF NOT EXISTS dev_post (
    id BIGINT PRIMARY KEY DEFAULT nextval('dev_post_snowflake_seq'),
    ...  -- all remaining columns/tables/indexes exactly as in Step 1
);
```
Confirm it is a regular file: `ls -la migrations-test/0037_dev_post.sql` shows `-rw-…`, not `l…`.

- [ ] **Step 4: Verify migrations apply in tests**

Add a throwaway smoke test at `src/controllers/dev_post/mod.rs` (created here as a stub with only the test), then run it:
```rust
#[cfg(test)]
mod migration_smoke {
    #[sqlx::test(migrations = "./migrations-test")]
    async fn dev_post_tables_exist(pool: sqlx::PgPool) {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM information_schema.tables
             WHERE table_name IN
             ('dev_post','dev_post_image','dev_post_poll','dev_post_poll_option','dev_post_like','dev_post_poll_vote')"
        ).fetch_one(&pool).await.unwrap();
        assert_eq!(n, 6);
    }
}
```
Run: `cargo test -p api-server dev_post_tables_exist` (adjust crate name to the actual package in `Cargo.toml`).
Expected: PASS (6 tables). If it fails on `pgactive.…`, you edited the test copy wrong — it must use `nextval`.

- [ ] **Step 5: Commit** (two repos)

```bash
# in migrations submodule
cd migrations && git checkout -b feat/dev-post-tables v2
git add 0037_dev_post.sql v2_upgrade_new_tables.sql
git commit -m "feat: add dev_post tables (pgactive snowflake pk, natural composite keys)"
git push -u origin feat/dev-post-tables && cd ..
# parent repo: gitlink bump + test-only file
git add migrations migrations-test/0037_dev_post.sql src/controllers/dev_post/mod.rs
git commit -m "feat(dev-post): schema migrations + test copy"
```

---

## Task 2: Types + validation

**Files:**
- Create/replace: `src/types/dev_post/mod.rs`
- Modify: `src/types/mod.rs` (add `pub mod dev_post;`)

**Interfaces:**
- Produces (request):
  - `CreateDevPostRequest { token_id: String, body: Option<String>, image_uris: Option<Vec<String>>, poll: Option<CreatePollRequest> }`, `fn validate(&self) -> Result<(), String>`
  - `CreatePollRequest { options: Vec<CreatePollOptionRequest> }`
  - `CreatePollOptionRequest { label: String, image_uri: Option<String> }`
  - `EditDevPostRequest { body: Option<String>, image_uris: Option<Vec<String>> }`, `fn validate(&self) -> Result<(), String>`
  - `VoteRequest { option_position: i16 }`
- Produces (response): `DevPostResponse`, `TokenSummary`, `AuthorSummary`, `PollResponse`, `PollOptionResponse`, `DevPostListResponse`, `TrendingResponse`, `RankingRow`, `RankingResponse`, `LikeResponse`, `VoteResponse`, `UploadImageResponse`.
- Produces (const): `MAX_IMAGES: usize = 4`, `MIN_POLL_OPTIONS: usize = 2`, `MAX_POLL_OPTIONS: usize = 3`, `POLL_DURATION_DAYS: i64 = 14`.

- [ ] **Step 1: Write failing unit tests** in `src/types/dev_post/mod.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn opt(label: &str) -> CreatePollOptionRequest { CreatePollOptionRequest { label: label.into(), image_uri: None } }

    #[test]
    fn rejects_completely_empty_post() {
        let r = CreateDevPostRequest { token_id: "0x0".into(), body: None, image_uris: None, poll: None };
        assert!(r.validate().is_err());
    }
    #[test]
    fn accepts_body_only() {
        let r = CreateDevPostRequest { token_id: "0x0".into(), body: Some("gm".into()), image_uris: None, poll: None };
        assert!(r.validate().is_ok());
    }
    #[test]
    fn rejects_too_many_images() {
        let imgs = vec!["u".to_string(); MAX_IMAGES + 1];
        let r = CreateDevPostRequest { token_id: "0x0".into(), body: Some("x".into()), image_uris: Some(imgs), poll: None };
        assert!(r.validate().is_err());
    }
    #[test]
    fn rejects_poll_with_one_option() {
        let r = CreateDevPostRequest { token_id: "0x0".into(), body: Some("x".into()), image_uris: None,
            poll: Some(CreatePollRequest { options: vec![opt("A")] }) };
        assert!(r.validate().is_err());
    }
    #[test]
    fn rejects_poll_with_four_options() {
        let r = CreateDevPostRequest { token_id: "0x0".into(), body: Some("x".into()), image_uris: None,
            poll: Some(CreatePollRequest { options: vec![opt("A"), opt("B"), opt("C"), opt("D")] }) };
        assert!(r.validate().is_err());
    }
    #[test]
    fn rejects_poll_with_empty_label() {
        let r = CreateDevPostRequest { token_id: "0x0".into(), body: Some("x".into()), image_uris: None,
            poll: Some(CreatePollRequest { options: vec![opt("A"), opt("")] }) };
        assert!(r.validate().is_err());
    }
    #[test]
    fn accepts_poll_with_two_options() {
        let r = CreateDevPostRequest { token_id: "0x0".into(), body: None, image_uris: None,
            poll: Some(CreatePollRequest { options: vec![opt("A"), opt("B")] }) };
        assert!(r.validate().is_ok());
    }
    #[test]
    fn edit_rejects_too_many_images() {
        let r = EditDevPostRequest { body: None, image_uris: Some(vec!["u".to_string(); MAX_IMAGES + 1]) };
        assert!(r.validate().is_err());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail** — `cargo test dev_post::tests` → FAIL (types not defined).

- [ ] **Step 3: Implement the types + validate()**

```rust
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const MAX_IMAGES: usize = 4;
pub const MIN_POLL_OPTIONS: usize = 2;
pub const MAX_POLL_OPTIONS: usize = 3;
pub const POLL_DURATION_DAYS: i64 = 14;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePollOptionRequest { pub label: String, pub image_uri: Option<String> }

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePollRequest { pub options: Vec<CreatePollOptionRequest> }

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateDevPostRequest {
    pub token_id: String,
    pub body: Option<String>,
    pub image_uris: Option<Vec<String>>,
    pub poll: Option<CreatePollRequest>,
}

fn validate_images(image_uris: &Option<Vec<String>>) -> Result<(), String> {
    if let Some(imgs) = image_uris {
        if imgs.len() > MAX_IMAGES { return Err(format!("At most {MAX_IMAGES} images allowed")); }
        if imgs.iter().any(|u| u.trim().is_empty()) { return Err("Empty image_uri".into()); }
    }
    Ok(())
}

fn validate_poll(poll: &Option<CreatePollRequest>) -> Result<(), String> {
    if let Some(p) = poll {
        if p.options.len() < MIN_POLL_OPTIONS || p.options.len() > MAX_POLL_OPTIONS {
            return Err(format!("Poll must have {MIN_POLL_OPTIONS}..={MAX_POLL_OPTIONS} options"));
        }
        if p.options.iter().any(|o| o.label.trim().is_empty()) { return Err("Empty poll option label".into()); }
    }
    Ok(())
}

impl CreateDevPostRequest {
    pub fn validate(&self) -> Result<(), String> {
        let has_body = self.body.as_ref().map(|b| !b.trim().is_empty()).unwrap_or(false);
        let has_images = self.image_uris.as_ref().map(|v| !v.is_empty()).unwrap_or(false);
        let has_poll = self.poll.is_some();
        if !has_body && !has_images && !has_poll {
            return Err("Post must have a body, at least one image, or a poll".into());
        }
        validate_images(&self.image_uris)?;
        validate_poll(&self.poll)?;
        Ok(())
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EditDevPostRequest { pub body: Option<String>, pub image_uris: Option<Vec<String>> }
impl EditDevPostRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.body.is_none() && self.image_uris.is_none() {
            return Err("Nothing to update".into());
        }
        validate_images(&self.image_uris)
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VoteRequest { pub option_position: i16 }

// ---- responses ----
#[derive(Debug, Serialize, ToSchema)]
pub struct TokenSummary { pub token_id: String, pub name: String, pub symbol: String,
    pub image_uri: Option<String>, pub market_cap: Option<String> }
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthorSummary { pub account_id: String, pub nickname: Option<String>, pub image_uri: Option<String> }
#[derive(Debug, Serialize, ToSchema)]
pub struct PollOptionResponse { pub position: i16, pub label: String, pub image_uri: Option<String>, pub vote_count: i64 }
#[derive(Debug, Serialize, ToSchema)]
pub struct PollResponse { pub closes_at: chrono::DateTime<chrono::Utc>, pub is_closed: bool,
    pub total_votes: i64, pub options: Vec<PollOptionResponse>, pub my_vote_option: Option<i16> }
#[derive(Debug, Serialize, ToSchema)]
pub struct DevPostResponse {
    pub id: String,                    // BIGINT as string
    pub token: TokenSummary,
    pub author: AuthorSummary,
    pub body: String,
    pub tweet_url: Option<String>,
    pub images: Vec<String>,
    pub poll: Option<PollResponse>,
    pub like_count: i64,
    pub liked_by_me: bool,
    pub is_edited: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Debug, Serialize, ToSchema)]
pub struct DevPostListResponse { pub posts: Vec<DevPostResponse>, pub total_count: i64 }
#[derive(Debug, Serialize, ToSchema)]
pub struct TrendingResponse { pub posts: Vec<DevPostResponse> }
#[derive(Debug, Serialize, ToSchema)]
pub struct RankingRow { pub rank: i64, pub token: TokenSummary, pub total_likes: i64,
    pub post_count: i64, pub last_posted_at: Option<chrono::DateTime<chrono::Utc>> }
#[derive(Debug, Serialize, ToSchema)]
pub struct RankingResponse { pub rankings: Vec<RankingRow>, pub total_count: i64 }
#[derive(Debug, Serialize, ToSchema)]
pub struct LikeResponse { pub like_count: i64, pub liked_by_me: bool }
#[derive(Debug, Serialize, ToSchema)]
pub struct VoteResponse { pub total_votes: i64, pub options: Vec<PollOptionResponse>, pub my_vote_option: Option<i16> }
#[derive(Debug, Serialize, ToSchema)]
pub struct UploadImageResponse { pub image_uri: String }

/// Extract the first x.com/twitter.com URL from a post body (no network fetch).
pub fn parse_tweet_url(body: &str) -> Option<String> {
    body.split_whitespace()
        .find(|tok| {
            let t = tok.trim_end_matches(|c: char| matches!(c, '.' | ',' | ')' | ']' | '"' | '\''));
            (t.starts_with("https://x.com/") || t.starts_with("https://twitter.com/")
             || t.starts_with("https://www.x.com/") || t.starts_with("https://www.twitter.com/"))
        })
        .map(|t| t.trim_end_matches(|c: char| matches!(c, '.' | ',' | ')' | ']' | '"' | '\'')).to_string())
}
```
Add `#[test] fn parses_tweet_url()` asserting `parse_tweet_url("check https://x.com/a/status/1 !")` == `Some("https://x.com/a/status/1".into())` and `parse_tweet_url("no link")` == `None`; include it in Step 1's test module.

- [ ] **Step 4: Run tests** — `cargo test dev_post::tests` → PASS.

- [ ] **Step 5: Commit**
```bash
git add src/types/dev_post/mod.rs src/types/mod.rs
git commit -m "feat(dev-post): request/response types + validation"
```

---

## Task 3: Controller — create/edit/delete (writes)

**Files:**
- Modify: `src/controllers/dev_post/mod.rs`
- Test: same file, `#[cfg(test)]`.

**Interfaces:**
- Consumes: `CreateDevPostRequest`, `EditDevPostRequest`, `POLL_DURATION_DAYS` (Task 2); `PostgresDatabase` (`get_read_pool`/`get_write_pool`).
- Produces:
  - `DevPostController { db: Arc<PostgresDatabase> }`, `fn new(db: Arc<PostgresDatabase>) -> Self`
  - `async fn create_post(&self, author: &str, req: &CreateDevPostRequest) -> Result<i64, AppError>` (Forbidden if `author != token.creator`; NotFound if token missing)
  - `async fn edit_post(&self, post_id: i64, author: &str, req: &EditDevPostRequest) -> Result<(), AppError>` (NotFound if missing/deleted; Forbidden if not author)
  - `async fn delete_post(&self, post_id: i64, author: &str) -> Result<(), AppError>` (soft delete; NotFound/Forbidden)

- [ ] **Step 1: Write failing integration tests**

```rust
#[cfg(test)]
mod write_tests {
    use super::*;
    use crate::types::dev_post::*;
    use std::sync::Arc;

    async fn seed_token(pool: &sqlx::PgPool, token_id: &str, creator: &str) {
        // minimal token row for FK/creator checks — adjust columns to 0002_token.sql NOT NULLs
        sqlx::query("INSERT INTO token (token_id, creator, name, symbol) VALUES ($1,$2,'T','TKN')
                     ON CONFLICT (token_id) DO NOTHING")
            .bind(token_id).bind(creator).execute(pool).await.unwrap();
    }
    fn ctl(pool: sqlx::PgPool) -> DevPostController {
        DevPostController::new(Arc::new(crate::db::postgres::PostgresDatabase::from_single_pool(pool)))
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn create_rejects_non_creator(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool);
        let req = CreateDevPostRequest { token_id: "0xToken".into(), body: Some("hi".into()), image_uris: None, poll: None };
        let err = c.create_post("0xNotCreator", &req).await.unwrap_err();
        assert!(matches!(err, AppError::Forbidden(_)));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn create_with_images_and_poll(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let req = CreateDevPostRequest {
            token_id: "0xToken".into(), body: Some("gm https://x.com/a/status/1".into()),
            image_uris: Some(vec!["u1".into(), "u2".into()]),
            poll: Some(CreatePollRequest { options: vec![
                CreatePollOptionRequest { label: "A".into(), image_uri: None },
                CreatePollOptionRequest { label: "B".into(), image_uri: Some("ib".into()) }]}),
        };
        let id = c.create_post("0xCreator", &req).await.unwrap();
        let imgs: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_image WHERE post_id=$1").bind(id).fetch_one(&pool).await.unwrap();
        let opts: i64 = sqlx::query_scalar("SELECT count(*) FROM dev_post_poll_option WHERE post_id=$1").bind(id).fetch_one(&pool).await.unwrap();
        let closes: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) =
            sqlx::query_as("SELECT p.created_at, poll.closes_at FROM dev_post p JOIN dev_post_poll poll ON poll.post_id=p.id WHERE p.id=$1")
            .bind(id).fetch_one(&pool).await.unwrap();
        assert_eq!(imgs, 2); assert_eq!(opts, 2);
        assert_eq!((closes.1 - closes.0).num_days(), 14);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn edit_by_non_author_forbidden(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool);
        let id = c.create_post("0xCreator", &CreateDevPostRequest { token_id:"0xToken".into(), body:Some("x".into()), image_uris:None, poll:None }).await.unwrap();
        let err = c.edit_post(id, "0xSomeoneElse", &EditDevPostRequest { body: Some("y".into()), image_uris: None }).await.unwrap_err();
        assert!(matches!(err, AppError::Forbidden(_)));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn delete_soft_hides_post(pool: sqlx::PgPool) {
        seed_token(&pool, "0xToken", "0xCreator").await;
        let c = ctl(pool.clone());
        let id = c.create_post("0xCreator", &CreateDevPostRequest { token_id:"0xToken".into(), body:Some("x".into()), image_uris:None, poll:None }).await.unwrap();
        c.delete_post(id, "0xCreator").await.unwrap();
        let deleted: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar("SELECT deleted_at FROM dev_post WHERE id=$1").bind(id).fetch_one(&pool).await.unwrap();
        assert!(deleted.is_some());
    }
}
```

> **Note (test helper dependency):** `PostgresDatabase::from_single_pool(pool)` is a test constructor that sets both `read_pool` and `write_pool` to the same pool. If it does not exist, add it under `#[cfg(test)]` in `src/db/postgres/mod.rs` in this step (a 3-line constructor). Verify the exact `token` NOT NULL columns in `migrations/0002_token.sql` and adjust `seed_token` accordingly.

- [ ] **Step 2: Run tests to verify they fail** — `cargo test dev_post::write_tests` → FAIL (no `DevPostController`).

- [ ] **Step 3: Implement the controller writes**

```rust
use crate::db::postgres::PostgresDatabase;
use crate::result::AppError;
use crate::types::dev_post::{CreateDevPostRequest, EditDevPostRequest, POLL_DURATION_DAYS};
use std::sync::Arc;

pub struct DevPostController { db: Arc<PostgresDatabase> }

impl DevPostController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self { Self { db } }

    pub async fn create_post(&self, author: &str, req: &CreateDevPostRequest) -> Result<i64, AppError> {
        let pool = self.db.get_write_pool();
        let mut tx = pool.begin().await.map_err(|e| AppError::InternalError(e.to_string()))?;

        // creator authorization on the write pool (avoid replica lag)
        let creator: Option<String> = sqlx::query_scalar("SELECT creator FROM token WHERE token_id = $1")
            .bind(&req.token_id).fetch_optional(&mut *tx).await
            .map_err(|e| AppError::InternalError(e.to_string()))?;
        match creator {
            None => return Err(AppError::NotFound("Token not found".into())),
            Some(c) if c != author => return Err(AppError::Forbidden("Only the coin creator can post".into())),
            _ => {}
        }

        let id: i64 = sqlx::query_scalar(
            "INSERT INTO dev_post (token_id, author, body) VALUES ($1,$2,$3) RETURNING id")
            .bind(&req.token_id).bind(author).bind(req.body.clone().unwrap_or_default())
            .fetch_one(&mut *tx).await.map_err(|e| AppError::InternalError(e.to_string()))?;

        if let Some(imgs) = &req.image_uris {
            for (i, uri) in imgs.iter().enumerate() {
                sqlx::query("INSERT INTO dev_post_image (post_id, position, image_uri) VALUES ($1,$2,$3)")
                    .bind(id).bind(i as i16).bind(uri).execute(&mut *tx).await
                    .map_err(|e| AppError::InternalError(e.to_string()))?;
            }
        }
        if let Some(poll) = &req.poll {
            sqlx::query(
                "INSERT INTO dev_post_poll (post_id, closes_at)
                 SELECT id, created_at + ($2 || ' days')::interval FROM dev_post WHERE id = $1")
                .bind(id).bind(POLL_DURATION_DAYS.to_string()).execute(&mut *tx).await
                .map_err(|e| AppError::InternalError(e.to_string()))?;
            for (i, opt) in poll.options.iter().enumerate() {
                sqlx::query("INSERT INTO dev_post_poll_option (post_id, position, label, image_uri) VALUES ($1,$2,$3,$4)")
                    .bind(id).bind((i + 1) as i16).bind(&opt.label).bind(&opt.image_uri)
                    .execute(&mut *tx).await.map_err(|e| AppError::InternalError(e.to_string()))?;
            }
        }
        tx.commit().await.map_err(|e| AppError::InternalError(e.to_string()))?;
        Ok(id)
    }

    /// Returns the author if the post exists and is not deleted, else NotFound.
    async fn require_author<'e, E>(exec: E, post_id: i64) -> Result<String, AppError>
    where E: sqlx::PgExecutor<'e> {
        let row: Option<String> = sqlx::query_scalar(
            "SELECT author FROM dev_post WHERE id = $1 AND deleted_at IS NULL")
            .bind(post_id).fetch_optional(exec).await
            .map_err(|e| AppError::InternalError(e.to_string()))?;
        row.ok_or_else(|| AppError::NotFound("Post not found".into()))
    }

    pub async fn edit_post(&self, post_id: i64, author: &str, req: &EditDevPostRequest) -> Result<(), AppError> {
        let pool = self.db.get_write_pool();
        let mut tx = pool.begin().await.map_err(|e| AppError::InternalError(e.to_string()))?;
        let existing = Self::require_author(&mut *tx, post_id).await?;
        if existing != author { return Err(AppError::Forbidden("Not the post author".into())); }

        if let Some(body) = &req.body {
            sqlx::query("UPDATE dev_post SET body=$2, updated_at=NOW(), edited_at=NOW() WHERE id=$1")
                .bind(post_id).bind(body).execute(&mut *tx).await
                .map_err(|e| AppError::InternalError(e.to_string()))?;
        } else {
            sqlx::query("UPDATE dev_post SET updated_at=NOW(), edited_at=NOW() WHERE id=$1")
                .bind(post_id).execute(&mut *tx).await.map_err(|e| AppError::InternalError(e.to_string()))?;
        }
        if let Some(imgs) = &req.image_uris {   // full replacement
            sqlx::query("DELETE FROM dev_post_image WHERE post_id=$1").bind(post_id)
                .execute(&mut *tx).await.map_err(|e| AppError::InternalError(e.to_string()))?;
            for (i, uri) in imgs.iter().enumerate() {
                sqlx::query("INSERT INTO dev_post_image (post_id, position, image_uri) VALUES ($1,$2,$3)")
                    .bind(post_id).bind(i as i16).bind(uri).execute(&mut *tx).await
                    .map_err(|e| AppError::InternalError(e.to_string()))?;
            }
        }
        tx.commit().await.map_err(|e| AppError::InternalError(e.to_string()))?;
        Ok(())
    }

    pub async fn delete_post(&self, post_id: i64, author: &str) -> Result<(), AppError> {
        let pool = self.db.get_write_pool();
        let existing = Self::require_author(pool, post_id).await?;
        if existing != author { return Err(AppError::Forbidden("Not the post author".into())); }
        sqlx::query("UPDATE dev_post SET deleted_at=NOW() WHERE id=$1")
            .bind(post_id).execute(pool).await.map_err(|e| AppError::InternalError(e.to_string()))?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run tests** — `cargo test dev_post::write_tests` → PASS. Also remove the Task 1 `migration_smoke` module or keep it; both green.

- [ ] **Step 5: Commit**
```bash
git add src/controllers/dev_post/mod.rs src/controllers/mod.rs src/db/postgres/mod.rs
git commit -m "feat(dev-post): controller create/edit/delete with creator auth"
```

---

## Task 4: Controller — like/unlike (derived count)

**Files:** Modify `src/controllers/dev_post/mod.rs`.

**Interfaces:**
- Produces:
  - `async fn like(&self, post_id: i64, account_id: &str) -> Result<i64, AppError>` (NotFound if post missing/deleted; returns fresh count)
  - `async fn unlike(&self, post_id: i64, account_id: &str) -> Result<i64, AppError>` (returns fresh count)
  - `async fn like_count(&self, post_id: i64) -> Result<i64, AppError>` (read pool)

- [ ] **Step 1: Failing tests**

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn like_is_idempotent_toggle(pool: sqlx::PgPool) {
    seed_token(&pool, "0xToken", "0xCreator").await;
    let c = ctl(pool.clone());
    let id = c.create_post("0xCreator", &CreateDevPostRequest{token_id:"0xToken".into(),body:Some("x".into()),image_uris:None,poll:None}).await.unwrap();
    assert_eq!(c.like(id, "0xUser").await.unwrap(), 1);
    assert_eq!(c.like(id, "0xUser").await.unwrap(), 1);      // idempotent, still 1
    assert_eq!(c.like(id, "0xUser2").await.unwrap(), 2);
    assert_eq!(c.unlike(id, "0xUser").await.unwrap(), 1);
    assert_eq!(c.unlike(id, "0xUser").await.unwrap(), 1);    // idempotent
}
#[sqlx::test(migrations = "./migrations-test")]
async fn like_deleted_post_404(pool: sqlx::PgPool) {
    seed_token(&pool, "0xToken", "0xCreator").await;
    let c = ctl(pool.clone());
    let id = c.create_post("0xCreator", &CreateDevPostRequest{token_id:"0xToken".into(),body:Some("x".into()),image_uris:None,poll:None}).await.unwrap();
    c.delete_post(id, "0xCreator").await.unwrap();
    assert!(matches!(c.like(id, "0xUser").await.unwrap_err(), AppError::NotFound(_)));
}
```

- [ ] **Step 2: Run → FAIL.**

- [ ] **Step 3: Implement**

```rust
impl DevPostController {
    pub async fn like(&self, post_id: i64, account_id: &str) -> Result<i64, AppError> {
        let pool = self.db.get_write_pool();
        // guard: post exists & not deleted (FK would allow liking a deleted post otherwise)
        let exists: Option<i64> = sqlx::query_scalar("SELECT id FROM dev_post WHERE id=$1 AND deleted_at IS NULL")
            .bind(post_id).fetch_optional(pool).await.map_err(|e| AppError::InternalError(e.to_string()))?;
        if exists.is_none() { return Err(AppError::NotFound("Post not found".into())); }
        sqlx::query("INSERT INTO dev_post_like (post_id, account_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
            .bind(post_id).bind(account_id).execute(pool).await.map_err(|e| AppError::InternalError(e.to_string()))?;
        self.count_likes(pool, post_id).await
    }
    pub async fn unlike(&self, post_id: i64, account_id: &str) -> Result<i64, AppError> {
        let pool = self.db.get_write_pool();
        sqlx::query("DELETE FROM dev_post_like WHERE post_id=$1 AND account_id=$2")
            .bind(post_id).bind(account_id).execute(pool).await.map_err(|e| AppError::InternalError(e.to_string()))?;
        self.count_likes(pool, post_id).await
    }
    pub async fn like_count(&self, post_id: i64) -> Result<i64, AppError> {
        self.count_likes(self.db.get_read_pool(), post_id).await
    }
    async fn count_likes<'e, E: sqlx::PgExecutor<'e>>(&self, exec: E, post_id: i64) -> Result<i64, AppError> {
        sqlx::query_scalar("SELECT count(*) FROM dev_post_like WHERE post_id=$1")
            .bind(post_id).fetch_one(exec).await.map_err(|e| AppError::InternalError(e.to_string()))
    }
}
```

- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** — `git commit -am "feat(dev-post): like/unlike toggle with derived count"`

---

## Task 5: Controller — vote (poll open check + change)

**Files:** Modify `src/controllers/dev_post/mod.rs`.

**Interfaces:**
- Produces: `async fn vote(&self, post_id: i64, account_id: &str, option_position: i16) -> Result<(), AppError>` — `Conflict` if poll closed; `NotFound` if no poll/post deleted; `BadRequest` if option_position not in poll.

- [ ] **Step 1: Failing tests**

```rust
async fn create_poll_post(c: &DevPostController) -> i64 {
    c.create_post("0xCreator", &CreateDevPostRequest{ token_id:"0xToken".into(), body:None, image_uris:None,
        poll: Some(CreatePollRequest{ options: vec![
            CreatePollOptionRequest{label:"A".into(),image_uri:None},
            CreatePollOptionRequest{label:"B".into(),image_uri:None}]})}).await.unwrap()
}
#[sqlx::test(migrations = "./migrations-test")]
async fn vote_then_change(pool: sqlx::PgPool) {
    seed_token(&pool, "0xToken", "0xCreator").await;
    let c = ctl(pool.clone());
    let id = create_poll_post(&c).await;
    c.vote(id, "0xUser", 1).await.unwrap();
    c.vote(id, "0xUser", 2).await.unwrap();   // change
    let (opt, n): (i16, i64) = sqlx::query_as("SELECT option_position, count(*) OVER () FROM dev_post_poll_vote WHERE post_id=$1 AND account_id=$2").bind(id).bind("0xUser").fetch_one(&pool).await.unwrap();
    assert_eq!(opt, 2); assert_eq!(n, 1);   // still exactly one vote row
}
#[sqlx::test(migrations = "./migrations-test")]
async fn vote_bad_option_400(pool: sqlx::PgPool) {
    seed_token(&pool, "0xToken", "0xCreator").await;
    let c = ctl(pool.clone());
    let id = create_poll_post(&c).await;
    assert!(matches!(c.vote(id, "0xUser", 9).await.unwrap_err(), AppError::BadRequest(_)));
}
#[sqlx::test(migrations = "./migrations-test")]
async fn vote_closed_poll_409(pool: sqlx::PgPool) {
    seed_token(&pool, "0xToken", "0xCreator").await;
    let c = ctl(pool.clone());
    let id = create_poll_post(&c).await;
    sqlx::query("UPDATE dev_post_poll SET closes_at = NOW() - INTERVAL '1 day' WHERE post_id=$1").bind(id).execute(&pool).await.unwrap();
    assert!(matches!(c.vote(id, "0xUser", 1).await.unwrap_err(), AppError::Conflict(_)));
}
```

- [ ] **Step 2: Run → FAIL.**

- [ ] **Step 3: Implement**

```rust
impl DevPostController {
    pub async fn vote(&self, post_id: i64, account_id: &str, option_position: i16) -> Result<(), AppError> {
        let pool = self.db.get_write_pool();
        let mut tx = pool.begin().await.map_err(|e| AppError::InternalError(e.to_string()))?;
        let closes_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "SELECT poll.closes_at FROM dev_post_poll poll JOIN dev_post p ON p.id = poll.post_id
             WHERE poll.post_id = $1 AND p.deleted_at IS NULL")
            .bind(post_id).fetch_optional(&mut *tx).await.map_err(|e| AppError::InternalError(e.to_string()))?;
        let closes_at = closes_at.ok_or_else(|| AppError::NotFound("Poll not found".into()))?;
        if closes_at <= chrono::Utc::now() { return Err(AppError::Conflict("Poll is closed".into())); }

        let opt_exists: Option<i16> = sqlx::query_scalar(
            "SELECT position FROM dev_post_poll_option WHERE post_id=$1 AND position=$2")
            .bind(post_id).bind(option_position).fetch_optional(&mut *tx).await
            .map_err(|e| AppError::InternalError(e.to_string()))?;
        if opt_exists.is_none() { return Err(AppError::BadRequest("Invalid option_position".into())); }

        sqlx::query(
            "INSERT INTO dev_post_poll_vote (post_id, account_id, option_position) VALUES ($1,$2,$3)
             ON CONFLICT (post_id, account_id)
             DO UPDATE SET option_position = EXCLUDED.option_position, updated_at = NOW()")
            .bind(post_id).bind(account_id).bind(option_position).execute(&mut *tx).await
            .map_err(|e| AppError::InternalError(e.to_string()))?;
        tx.commit().await.map_err(|e| AppError::InternalError(e.to_string()))?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** — `git commit -am "feat(dev-post): poll vote with open-check and change support"`

---

## Task 6: Controller — reads (feed, detail, trending, ranking + hydration)

**Files:** Modify `src/controllers/dev_post/mod.rs`.

**Interfaces:**
- Consumes: `PaginationParams` (`src/types/common/pagination.rs`: `page: i64`, `limit: i64`, `direction: String`).
- Produces:
  - `async fn get_feed(&self, token_id: Option<&str>, page: i64, limit: i64, viewer: Option<&str>) -> Result<(Vec<DevPostResponse>, i64), AppError>`
  - `async fn get_post(&self, post_id: i64, viewer: Option<&str>) -> Result<DevPostResponse, AppError>` (NotFound if missing/deleted)
  - `async fn get_trending(&self, viewer: Option<&str>) -> Result<Vec<DevPostResponse>, AppError>`
  - `async fn get_ranking(&self, page: i64, limit: i64) -> Result<(Vec<RankingRow>, i64), AppError>`

**Approach (hydration):** select the page of `dev_post` ids first, then batch-load images, poll+options+vote counts, like counts, and viewer-personalization for those ids, and assemble `DevPostResponse` in Rust (`parse_tweet_url(body)` for `tweet_url`, `is_edited = edited_at.is_some()`). This keeps each query indexed and avoids N+1.

- [ ] **Step 1: Failing tests** (behavior-level; exact SQL asserted via outcomes)

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn feed_excludes_deleted_and_orders_newest_first(pool: sqlx::PgPool) {
    seed_token(&pool, "0xToken", "0xCreator").await;
    let c = ctl(pool.clone());
    let a = c.create_post("0xCreator", &CreateDevPostRequest{token_id:"0xToken".into(),body:Some("a".into()),image_uris:None,poll:None}).await.unwrap();
    let b = c.create_post("0xCreator", &CreateDevPostRequest{token_id:"0xToken".into(),body:Some("b".into()),image_uris:None,poll:None}).await.unwrap();
    c.delete_post(a, "0xCreator").await.unwrap();
    let (posts, total) = c.get_feed(Some("0xToken"), 1, 10, None).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(posts[0].id, b.to_string());
    assert!(!posts[0].liked_by_me);
}
#[sqlx::test(migrations = "./migrations-test")]
async fn detail_personalization_and_poll(pool: sqlx::PgPool) {
    seed_token(&pool, "0xToken", "0xCreator").await;
    let c = ctl(pool.clone());
    let id = create_poll_post(&c).await;      // from Task 5 tests (move helper to shared test mod)
    c.like(id, "0xU").await.unwrap();
    c.vote(id, "0xU", 2).await.unwrap();
    let r = c.get_post(id, Some("0xU")).await.unwrap();
    assert!(r.liked_by_me);
    assert_eq!(r.like_count, 1);
    let poll = r.poll.unwrap();
    assert_eq!(poll.my_vote_option, Some(2));
    assert_eq!(poll.total_votes, 1);
    assert!(!poll.is_closed);
}
#[sqlx::test(migrations = "./migrations-test")]
async fn ranking_and_trending_by_likes(pool: sqlx::PgPool) {
    seed_token(&pool, "0xToken", "0xCreator").await;
    let c = ctl(pool.clone());
    let p = c.create_post("0xCreator", &CreateDevPostRequest{token_id:"0xToken".into(),body:Some("p".into()),image_uris:None,poll:None}).await.unwrap();
    c.like(p, "0xU1").await.unwrap(); c.like(p, "0xU2").await.unwrap();
    let (rank, total) = c.get_ranking(1, 10).await.unwrap();
    assert_eq!(total, 1);
    assert_eq!(rank[0].total_likes, 2);
    assert_eq!(rank[0].post_count, 1);
    let trending = c.get_trending(None).await.unwrap();
    assert_eq!(trending.len(), 1);
    assert_eq!(trending[0].id, p.to_string());
}
```

- [ ] **Step 2: Run → FAIL.**

- [ ] **Step 3: Implement reads.** Key SQL (implement helpers `hydrate(ids, viewer)`; token/author joins reuse `token`+`account`):

Feed page ids + count:
```sql
-- ids (with optional token filter)
SELECT id FROM dev_post
WHERE deleted_at IS NULL AND ($1::varchar IS NULL OR token_id = $1)
ORDER BY id DESC LIMIT $2 OFFSET $3;
-- total_count
SELECT count(*) FROM dev_post WHERE deleted_at IS NULL AND ($1::varchar IS NULL OR token_id = $1);
```
Trending ids (last 7 days likes, top 3):
```sql
SELECT l.post_id FROM dev_post_like l JOIN dev_post p ON p.id = l.post_id
WHERE p.deleted_at IS NULL AND l.created_at >= NOW() - INTERVAL '7 days'
GROUP BY l.post_id ORDER BY count(*) DESC, l.post_id DESC LIMIT 3;
```
Ranking page (per coin) + count:
```sql
SELECT p.token_id,
       COUNT(l.account_id)                    AS total_likes,
       COUNT(DISTINCT p.id)                   AS post_count,
       MAX(p.created_at)                      AS last_posted_at
FROM dev_post p
LEFT JOIN dev_post_like l ON l.post_id = p.id
WHERE p.deleted_at IS NULL
GROUP BY p.token_id
ORDER BY total_likes DESC, last_posted_at DESC
LIMIT $1 OFFSET $2;
-- count of coins with >=1 non-deleted post:
SELECT count(DISTINCT token_id) FROM dev_post WHERE deleted_at IS NULL;
```
Per-page hydration (bind `ids: &[i64]` with `= ANY($1)`):
```sql
-- posts core + token + author
SELECT dp.id, dp.token_id, dp.author, dp.body, dp.created_at, dp.updated_at, dp.edited_at,
       t.name AS token_name, t.symbol AS token_symbol, t.image_uri AS token_image,
       a.nickname AS author_nickname, a.image_uri AS author_image
FROM dev_post dp
JOIN token t   ON t.token_id = dp.token_id
LEFT JOIN account a ON a.account_id = dp.author
WHERE dp.id = ANY($1);
-- images
SELECT post_id, position, image_uri FROM dev_post_image WHERE post_id = ANY($1) ORDER BY post_id, position;
-- like counts
SELECT post_id, count(*) AS n FROM dev_post_like WHERE post_id = ANY($1) GROUP BY post_id;
-- viewer liked (only when viewer is Some)
SELECT post_id FROM dev_post_like WHERE post_id = ANY($1) AND account_id = $2;
-- polls
SELECT post_id, closes_at FROM dev_post_poll WHERE post_id = ANY($1);
-- options + vote counts
SELECT o.post_id, o.position, o.label, o.image_uri, COUNT(v.account_id) AS votes
FROM dev_post_poll_option o
LEFT JOIN dev_post_poll_vote v ON v.post_id = o.post_id AND v.option_position = o.position
WHERE o.post_id = ANY($1) GROUP BY o.post_id, o.position, o.label, o.image_uri ORDER BY o.post_id, o.position;
-- viewer vote (only when viewer is Some)
SELECT post_id, option_position FROM dev_post_poll_vote WHERE post_id = ANY($1) AND account_id = $2;
```
Assemble in Rust: build `DevPostResponse` per id preserving the id-order from the page query; `market_cap` = `None` for now (Task 6b wires the real source — see Global note); `tweet_url = parse_tweet_url(&body)`; `is_closed = closes_at <= Utc::now()`; `total_votes = sum(option votes)`. Offset = `(page-1)*limit`.

> **Market cap:** leave `TokenSummary.market_cap = None` in this task and open a follow-up step **Task 6b** to join the existing market-cap source once confirmed (design §8). Do not block feed/detail on it. Add a `// TODO(market_cap): join market source` is **not** allowed — instead ship `None` and track 6b as a real task below.

- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** — `git commit -am "feat(dev-post): read paths (feed/detail/trending/ranking) with hydration"`

### Task 6b: Wire real market cap into TokenSummary
- [ ] **Step 1:** In planning you confirmed the market-cap column/view (design §8 dependency). Add its value to the hydration "posts core" query (`SELECT … m.market_cap` joined from the market source used by `src/controllers/token/metadata.rs`) and to the ranking token join. Serialize as string.
- [ ] **Step 2:** Extend `ranking_and_trending_by_likes` / `detail_*` tests to assert `token.market_cap.is_some()` when a market row is seeded.
- [ ] **Step 3:** Run → PASS. Commit `feat(dev-post): populate market_cap in token summary`.

---

## Task 7: R2 image upload method

**Files:** Modify `src/db/r2/mod.rs`.

**Interfaces:**
- Produces: `async fn upload_devpost_image_file(&self, image_id: &str, body: &Bytes, content_type: &str) -> anyhow::Result<String>` returning `https://storage.nadapp.net/devpost/{image_id}`.

- [ ] **Step 1:** Copy `upload_account_image_file` (lines 199-240) to a new `upload_devpost_image_file`, changing only the key to `format!("devpost/{}", image_id)` and log messages to "devpost image".
- [ ] **Step 2:** `cargo build` → compiles.
- [ ] **Step 3: Commit** — `git commit -am "feat(dev-post): R2 upload_devpost_image_file"`

---

## Task 8: `optional_session_address` helper

**Files:** Modify `src/middleware.rs`.

**Interfaces:**
- Produces: `pub async fn optional_session_address(state: &AppState, headers: &HeaderMap, cookies: &Cookies) -> Option<String>` — mirrors the cookie→address resolution in `authenticate_user` (Redis fast path → Postgres fallback) but returns `None` instead of erroring when the cookie is missing/invalid. (Match the exact extractor types `authenticate_user` uses for cookies; if it reads `tower_cookies::Cookies`, take that.)

- [ ] **Step 1:** Extract the cookie-name lookup + `redis.get_address_by_session` + `SessionController::get_address_by_session_id` fallback from `authenticate_user` into a private `resolve_session_address(...) -> Option<String>` and have both `authenticate_user` (401 on `None`) and `optional_session_address` (returns `None`) call it. Do not duplicate logic (DRY).
- [ ] **Step 2:** `cargo build` → compiles; existing auth tests still pass (`cargo test middleware`).
- [ ] **Step 3: Commit** — `git commit -am "refactor(auth): share session resolution; add optional_session_address"`

---

## Task 9: Service layer + Redis cache

**Files:** Create `src/services/dev_post/mod.rs`; modify `src/services/mod.rs`.

**Interfaces:**
- Produces: `DevPostService { postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>, r2: Arc<R2Client> }`, `fn new(...)`, and pass-throughs: `create_post`, `edit_post`, `delete_post`, `like`, `unlike`, `vote`, `get_feed`, `get_post`, `upload_image`, plus cached `get_trending`, `get_ranking`.
- Cache: `get_trending`/`get_ranking` read Redis first (key `devpost:trending`, `devpost:ranking:{page}:{limit}`, TTL 60s); on miss, call controller + set. Invalidate (`DEL devpost:trending`, and the ranking keys) inside `create_post`/`delete_post`/`like`/`unlike`.

- [ ] **Step 1: Failing test** — a `#[sqlx::test]` that calls `service.get_trending` twice and asserts equal results (cache correctness), and that after a new like the trending set updates once the key is invalidated. (If Redis is not available in tests, gate the cache behind an `Option<Arc<RedisDatabase>>` and pass `None` in tests, asserting pass-through equals controller output — pick whichever matches how `HypeService` is tested; follow that precedent.)
- [ ] **Step 2: Run → FAIL.**
- [ ] **Step 3: Implement** following `src/services/hype/mod.rs` caching+invalidation pattern (Redis get → miss → controller → set; delete keys on writes). Add Redis helpers if a generic get/set-with-ttl exists; else reuse the JSON string pattern used by hype.
- [ ] **Step 4: Run → PASS.**
- [ ] **Step 5: Commit** — `git commit -am "feat(dev-post): service layer with trending/ranking cache"`

---

## Task 10: Router + handlers

**Files:** Create `src/router/dev_post/{path.rs, handler.rs, mod.rs}`; modify `src/router/mod.rs`.

**Interfaces:**
- Consumes: `DevPostService`, all types, `valid_existing_token_id`/`valid_account_id`, `Extension<String>` (session address), `optional_session_address`, `PaginationParams`.
- Produces: `pub fn router(app_state: AppState) -> Router<AppState>`.

- [ ] **Step 1: `path.rs`** — enum with `as_str`/`docs_str` (statics before capture):
```rust
pub enum DevPostPath { Trending, Ranking, UploadImage, Feed, Create, Detail, Edit, Delete, Like, Vote }
impl DevPostPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            DevPostPath::Trending => "/dev-post/trending",
            DevPostPath::Ranking  => "/dev-post/ranking",
            DevPostPath::UploadImage => "/dev-post/image",
            DevPostPath::Feed | DevPostPath::Create => "/dev-post",
            DevPostPath::Detail | DevPostPath::Edit | DevPostPath::Delete => "/dev-post/{post_id}",
            DevPostPath::Like | DevPostPath::Vote => "/dev-post/{post_id}/like", // Vote overrides below
        }
    }
}
```
(For `Vote` use `/dev-post/{post_id}/vote`; split the match arms so each path is exact. `docs_str` mirrors `as_str` with axum→utoipa `{post_id}` syntax.)

- [ ] **Step 2: `handler.rs`** — thin handlers. Representative bodies:

```rust
// POST /dev-post  (protected)
pub async fn create_post(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(mut payload): Json<CreateDevPostRequest>,
) -> AppJsonResult<DevPostResponse> {
    payload.validate().map_err(AppError::BadRequest)?;
    payload.token_id = valid_existing_token_id(&state, &payload.token_id).await?;
    let service = DevPostService::new(state.postgres.clone(), state.redis.clone(), state.r2.clone());
    let id = service.create_post(&session_address, &payload).await?;
    Ok(Json(service.get_post(id, Some(&session_address)).await?))
}

// GET /dev-post?token_id=&page=&limit=  (public, optional-auth)
pub async fn get_feed(
    State(state): State<AppState>,
    headers: HeaderMap, cookies: Cookies,
    Query(params): Query<FeedQuery>,        // { token_id: Option<String>, #[serde(flatten)] page: PaginationParams }
) -> AppJsonResult<DevPostListResponse> {
    let token_id = match &params.token_id { Some(t) => Some(valid_existing_token_id(&state, t).await?), None => None };
    let viewer = optional_session_address(&state, &headers, &cookies).await;
    let service = DevPostService::new(state.postgres.clone(), state.redis.clone(), state.r2.clone());
    let (posts, total_count) = service.get_feed(token_id.as_deref(), params.page.page, params.page.limit, viewer.as_deref()).await?;
    Ok(Json(DevPostListResponse { posts, total_count }))
}

// POST /dev-post/{post_id}/like  (protected)
pub async fn like(
    State(state): State<AppState>, Extension(session_address): Extension<String>,
    Path(post_id): Path<i64>,
) -> AppJsonResult<LikeResponse> {
    let service = DevPostService::new(state.postgres.clone(), state.redis.clone(), state.r2.clone());
    let like_count = service.like(post_id, &session_address).await?;
    Ok(Json(LikeResponse { like_count, liked_by_me: true }))
}
// DELETE /dev-post/{post_id}/like → service.unlike, liked_by_me: false
// POST /dev-post/{post_id}/vote → validate option, service.vote, then return VoteResponse from get_post's poll
// PATCH/DELETE /dev-post/{post_id} → validate + service.edit_post/delete_post
// POST /dev-post/image → Bytes body + content-type header (mirror src/router/account/handler.rs:230-279),
//     reuse metadata magic-byte+NSFW validation, uuid, service.upload_image → UploadImageResponse
// GET /dev-post/{post_id} → optional-auth → service.get_post
// GET /dev-post/trending, /dev-post/ranking → optional-auth (trending personalization), service cached calls
```
Define `FeedQuery { token_id: Option<String>, #[serde(flatten)] page: PaginationParams }` in `handler.rs` or types.

- [ ] **Step 3: `mod.rs`** — router with per-route auth layer (mirror `src/router/account/mod.rs:15-48`):
```rust
pub fn router(app_state: AppState) -> Router<AppState> {
    let auth = ServiceBuilder::new().layer(axum_middleware::from_fn_with_state(app_state, authenticate_user));
    Router::new()
        // public
        .route(DevPostPath::Trending.as_str(), get(handler::get_trending))
        .route(DevPostPath::Ranking.as_str(),  get(handler::get_ranking))
        .route(DevPostPath::Feed.as_str(),     get(handler::get_feed))
        .route(DevPostPath::Detail.as_str(),   get(handler::get_detail))
        // protected
        .route(DevPostPath::UploadImage.as_str(), post(handler::upload_image).layer(DefaultBodyLimit::max(5_000_000)).layer(auth.clone()))
        .route(DevPostPath::Create.as_str(),   post(handler::create_post).layer(auth.clone()))
        .route(DevPostPath::Edit.as_str(),     patch(handler::edit_post).layer(auth.clone()))
        .route(DevPostPath::Delete.as_str(),   delete(handler::delete_post).layer(auth.clone()))
        .route(DevPostPath::Like.as_str(),     post(handler::like).delete(handler::unlike).layer(auth.clone()))
        .route(DevPostPath::Vote.as_str(),     post(handler::vote).layer(auth.clone()))
}
```
> **Route conflict note:** `/dev-post` has both a public GET (feed) and protected POST (create) — attach the auth layer to the POST only. axum requires these on the same `.route(...)` call or via `MethodRouter`; combine as `get(get_feed).post(create_post.layer(auth))` if the per-method layer form is needed. Verify against how `hype`/`account` mix methods on one path; follow that exact form.

- [ ] **Step 4:** `cargo build` → compiles. `cargo test` (all dev_post tests) → PASS.
- [ ] **Step 5: Commit** — `git commit -am "feat(dev-post): router + handlers"`

---

## Task 11: Wire into app + OpenAPI

**Files:** Modify `src/main.rs`, `src/router/mod.rs`.

- [ ] **Step 1:** `src/router/mod.rs`: add `pub mod dev_post;`.
- [ ] **Step 2:** `src/main.rs`: add `.merge(dev_post::router(app_state.clone()))` in the app builder (near the other `.merge(...)` calls, ~line 518-560).
- [ ] **Step 3:** `src/main.rs` utoipa block (~line 42-414): add every handler to `paths(...)` and every response/request type + `RankingRow`, `PollResponse`, etc. to `components(schemas(...))`. Add `#[utoipa::path(...)]` annotations on each handler (copy the shape from `src/router/hype/handler.rs`).
- [ ] **Step 4:** `cargo build`; start server locally if feasible and hit `GET /dev-scalar` to confirm the Dev Post group renders. `cargo test` green.
- [ ] **Step 5: Commit** — `git commit -am "feat(dev-post): wire router + openapi docs"`

---

## Task 12: API reference doc

**Files:** Create `docs/dev-post-api.md`.

- [ ] **Step 1:** Write the reference following `docs/hype-api.md` / `docs/x-verification-api.md` style: each endpoint's method/path/auth, request JSON, response JSON (using the shapes in the design spec §3), and error table (§5). Note BIGINT ids are strings and poll close is read-time.
- [ ] **Step 2:** Update `docs/V2_API_CHANGES.md` with a "Dev Post API (new)" entry (memory: update docs before PR).
- [ ] **Step 3: Commit** — `git commit -am "docs(dev-post): API reference + V2_API_CHANGES entry"`

---

## Self-Review (completed by plan author)

**Spec coverage:** feed/trending/ranking/detail (Task 6), create/edit/delete (Task 3), like toggle (Task 4), vote+change+close (Task 5), image upload 2-step (Tasks 7+10), optional-auth personalization (Task 8, used in 6/10), tweet parse (Task 2 `parse_tweet_url`, used in 6), pgactive PKs + migrations dual-track + test divergence (Task 1), creator auth on write pool (Task 3), derived counts (Tasks 4/6), Redis cache (Task 9), OpenAPI + docs (Tasks 11/12). Market cap dependency tracked as explicit Task 6b rather than a placeholder.

**Placeholder scan:** No "TBD/TODO/handle appropriately" left in code steps; the one spot needing external confirmation (market cap) is a real, testable task (6b) shipping `None` until wired.

**Type consistency:** `DevPostController` method names (`create_post/edit_post/delete_post/like/unlike/vote/get_feed/get_post/get_trending/get_ranking/like_count`), types (`DevPostResponse`, `LikeResponse`, `VoteResponse`, `RankingRow`, `PollResponse`, `CreateDevPostRequest`, `EditDevPostRequest`, `VoteRequest`), and constants (`MAX_IMAGES`, `POLL_DURATION_DAYS`) are used identically across Tasks 2–11.

**Known verification points for the executor (not placeholders — confirm against live code):** exact `token` NOT NULL columns for `seed_token`; `PostgresDatabase` test constructor; the cookie extractor type `authenticate_user` uses; whether `token.token_id` has a UNIQUE constraint for the `dev_post.token_id` FK (if not, drop the FK and keep the column — the design still holds); per-method auth-layer syntax on shared paths.
