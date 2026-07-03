# X Hidden Creator Verification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a coin creator verify their X (Twitter) account via OAuth during coin creation and attach two *aggregate* social signals (their follower count, plus up to 3 handles that provably follow them) to a specific coin — without ever exposing the creator's real X handle.

**Architecture:** New `x_verification` domain (router/service/controller/types). Front-end drives a PKCE OAuth 2.0 flow; the server holds the creator's X token only in Redis (TTL) while the coin is being built ("pending"), then on `POST /x/verification/finalize` it re-derives the CREATE2 token address and checks `creator == session wallet` before persisting the signals to Postgres. The public `GET /token/:token` gains an `x_verification` field via a LEFT JOIN. The creator's X handle / X user-id are never returned in any response.

**Tech Stack:** Rust, Axum 0.7, sqlx 0.8 (Postgres), Redis (`redis` 0.29), `reqwest` 0.12, `alloy` 1.0 (CREATE2 / addresses), `sha2` + `base64` (PKCE), `utoipa` (OpenAPI). Design doc: `docs/plans/2026-07-03-x-hidden-creator-verification-design.md`.

## Global Constraints

- Integration/base branch is **`v2`**. This feature branch is `feat/x-hidden-creator-verification` (base `v2`).
- **Never expose** the creator's real `x_handle` or `x_user_id` in any API response. Only `followers_count` and the third-party `followed_by` handles (which are intentionally public) may be returned.
- EVM addresses compare by **EIP-55 checksum** (via `crate::utils::valid_account_id`, which returns `Address::to_checksum(None)`) or by `alloy::primitives::Address` byte-equality. **Never `LOWER()`-compare addresses.**
- `migrations/` is a **git submodule** (`git@github.com:Naddotfun/migrations.git`). New SQL goes on a submodule feature branch that is committed + pushed there; the parent repo records only a **gitlink bump**. Tests read `./migrations-test` (a plain parent-repo dir of symlinks/real files), so every new migration also needs a `migrations-test` entry.
- Redis one-time secrets are consumed with **GETDEL** (atomic, prevents replay); temporary data uses **PSETEX** (millisecond TTL). Mirror the existing `get_and_delete_sign_message` / `set_session` patterns in `src/db/redis/mod.rs`.
- External X API calls use a `Lazy<reqwest::Client>` singleton with a **5s timeout**, mirroring `src/services/pricing/defillama.rs`. On failure, return an error immediately (no retry, no caching).
- The OAuth callback redirect target is a **fixed server env URL** — never a client-supplied value (open-redirect prevention).
- X handle format is `^[A-Za-z0-9_]{1,15}$`, validated **before** any X API call.
- X API endpoints (verified in the `x-api` harness): authorize `https://x.com/i/oauth2/authorize`, token `https://api.x.com/2/oauth2/token`, base `https://api.x.com/2`, scopes `users.read tweet.read offline.access`, PKCE `code_challenge_method=S256`. `connection_status` containing `"followed_by"` ⇒ the looked-up user follows the authenticated creator.

---

## File Structure

**Create:**
- `migrations/0036_token_x_verification.sql` (submodule) — the two new tables.
- `migrations/v2_upgrade_token_x_verification.sql` (submodule) — prod upgrade track (identical `CREATE TABLE IF NOT EXISTS`).
- `migrations-test/0036_token_x_verification.sql` — symlink to the submodule file (test track).
- `src/types/x_verification/mod.rs` — request/response DTOs, Redis-serialized structs, pure helpers (`validate_handle`, `append_followed_by`).
- `src/types/token/x_verification.rs` — public response types (`TokenXVerification`, `XFollowedByEntry`).
- `src/services/x_oauth/client.rs` + `src/services/x_oauth/mod.rs` — X API client (PKCE, authorize URL, code exchange, get_me, check_follows_me).
- `src/services/x_verification/mod.rs` — orchestration; also `creator_matches` / `address_matches` finalize checks (pure, unit-tested).
- `src/controllers/x_verification/mod.rs` — Postgres persistence (finalize).
- `src/router/x_verification/{mod.rs,path.rs,handler.rs}` — 6 endpoints.

**Modify:**
- `src/config.rs` — new `lazy_static` env constants.
- `.env.example` — document the new env vars.
- `src/result.rs` — add `Forbidden` (403) and `Gone` (410) `AppError` variants.
- `src/services/token/salt.rs` — make `compute_create2_address` `pub(crate)`; add `pub fn compute_token_address(version, salt)`.
- `src/services/rate_limiter.rs` — add `X_OAUTH_LOGIN_RATE_LIMIT` / `X_FOLLOWED_BY_RATE_LIMIT` consts.
- `src/db/redis/mod.rs` — add X OAuth state + pending methods.
- `src/types/common/info.rs` — add `x_verification: Option<TokenXVerification>` to `TokenInfo`.
- All 20 `TokenInfo { … }` construction sites — add `x_verification: None` (only the token-detail site gets a real value).
- `src/controllers/token/mod.rs` — LEFT JOIN `token_x_verification`, fetch `followed_by`, populate `x_verification`.
- `src/services/mod.rs`, `src/controllers/mod.rs`, `src/types/mod.rs`, `src/types/token/mod.rs`, `src/router/mod.rs` — declare new sub-modules.
- `src/middleware.rs` — exclude `/x/oauth/callback` from `api_key_gate`.
- `src/main.rs` — import + `.merge()` the new router; register OpenAPI `paths` + `schemas`.

---

## Task 1: Migrations submodule + test track + gitlink bump

**Files:**
- Create: `migrations/0036_token_x_verification.sql` (submodule)
- Create: `migrations/v2_upgrade_token_x_verification.sql` (submodule)
- Create: `migrations-test/0036_token_x_verification.sql` (symlink)

**Interfaces:**
- Produces: tables `token_x_verification(token_id PK, account_id, x_user_id, followers_count, verified_at)` and `token_x_followed_by(token_id FK→token_x_verification, x_handle, x_image_uri, x_followers_count, checked_at, PK(token_id,x_handle))`. Consumed by Tasks 8 (controller) and 11 (token JOIN).

- [ ] **Step 1: Create a submodule feature branch**

```bash
git -C /Users/gyu/project/nads-pump/api-server/migrations checkout -b feat/token-x-verification
```

- [ ] **Step 2: Confirm 0036 is the next free number**

Run: `ls /Users/gyu/project/nads-pump/api-server/migrations | grep -E '^00' | sort | tail -3`
Expected: highest is `0035_whitelist_price_source_id.sql` (so `0036_` is free). If a `0036_*` already exists, use the next free `00NN`.

- [ ] **Step 3: Write the install migration** (design §4 DDL verbatim)

Create `/Users/gyu/project/nads-pump/api-server/migrations/0036_token_x_verification.sql`:

```sql
-- 0036_token_x_verification.sql
--
-- X (Twitter) hidden-creator verification signals, scoped to a single coin.
-- See: docs/plans/2026-07-03-x-hidden-creator-verification-design.md (§4)
--
-- No FK to `token`: at finalize time the token row does not exist yet
-- (the Observer indexer creates it later from the on-chain event).

-- Creator self-verification result (follower count). x_user_id is internal —
-- never exposed by the API.
CREATE TABLE IF NOT EXISTS token_x_verification (
    token_id VARCHAR(42) PRIMARY KEY,
    account_id VARCHAR(42) NOT NULL,
    x_user_id VARCHAR(32) NOT NULL,
    followers_count BIGINT NOT NULL,
    verified_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_token_x_verification_account_id
    ON token_x_verification (account_id);

-- Third-party handles that provably follow the creator (max 3, public).
-- Handles that do NOT follow are never stored.
CREATE TABLE IF NOT EXISTS token_x_followed_by (
    token_id VARCHAR(42) NOT NULL
        REFERENCES token_x_verification(token_id) ON DELETE CASCADE,
    x_handle VARCHAR(16) NOT NULL,
    x_image_uri VARCHAR NOT NULL,
    x_followers_count BIGINT NOT NULL,
    checked_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (token_id, x_handle)
);
```

- [ ] **Step 4: Write the prod upgrade-track migration** (identical, idempotent)

Create `/Users/gyu/project/nads-pump/api-server/migrations/v2_upgrade_token_x_verification.sql` with the **same** two `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` statements as Step 3 (copy them verbatim; `IF NOT EXISTS` makes it safe to run against a live DB).

- [ ] **Step 5: Commit + push the submodule branch**

```bash
git -C /Users/gyu/project/nads-pump/api-server/migrations add 0036_token_x_verification.sql v2_upgrade_token_x_verification.sql
git -C /Users/gyu/project/nads-pump/api-server/migrations commit -m "feat(x): token_x_verification + token_x_followed_by tables"
git -C /Users/gyu/project/nads-pump/api-server/migrations push -u origin feat/token-x-verification
```

- [ ] **Step 6: Wire the test track (symlink)**

```bash
ln -s ../migrations/0036_token_x_verification.sql \
  /Users/gyu/project/nads-pump/api-server/migrations-test/0036_token_x_verification.sql
```

Run: `ls -la /Users/gyu/project/nads-pump/api-server/migrations-test/0036_token_x_verification.sql`
Expected: a symlink `-> ../migrations/0036_token_x_verification.sql`.

- [ ] **Step 7: Verify the schema applies from scratch**

Run: `cargo test --lib migrations_apply_smoke 2>&1 | tail -5` after adding this throwaway test to `src/controllers/x_verification/mod.rs` (or run any existing `#[sqlx::test(migrations = "./migrations-test")]` test) — the point is that `./migrations-test` including `0036` applies cleanly. A dedicated smoke test:

```rust
#[cfg(test)]
mod migrations_smoke {
    use sqlx::PgPool;
    #[sqlx::test(migrations = "./migrations-test")]
    async fn tables_exist(pool: PgPool) {
        let (a,): (bool,) = sqlx::query_as(
            "SELECT to_regclass('token_x_verification') IS NOT NULL AND to_regclass('token_x_followed_by') IS NOT NULL",
        ).fetch_one(&pool).await.unwrap();
        assert!(a, "both x-verification tables must exist");
    }
}
```

Run: `cargo test tables_exist -- --nocapture`
Expected: PASS (requires a local Postgres reachable by `#[sqlx::test]`, per the existing test setup).

- [ ] **Step 8: Bump the parent gitlink**

```bash
git -C /Users/gyu/project/nads-pump/api-server add migrations migrations-test
git -C /Users/gyu/project/nads-pump/api-server commit -m "chore(migrations): bump submodule for token_x_verification (#gitlink)"
```

(Do not delete the throwaway smoke test yet; Task 8 replaces it with real controller tests.)

---

## Task 2: Config env constants + `.env.example`

**Files:**
- Modify: `src/config.rs`
- Modify: `.env.example`
- Test: `src/config.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `config::{X_CLIENT_ID, X_CLIENT_SECRET, X_REDIRECT_URI, X_OAUTH_REDIRECT_SUCCESS_URL, X_OAUTH_REDIRECT_FAILURE_URL, X_OAUTH_STATE_TTL_MS, X_PENDING_TTL_MS, X_FOLLOWED_BY_MAX}` — consumed by Tasks 5, 7, 9.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` block at the bottom of `src/config.rs` (create the block if absent):

```rust
#[cfg(test)]
mod config_tests {
    use super::*;
    #[test]
    fn followed_by_max_defaults_to_three() {
        // X_FOLLOWED_BY_MAX is unset in the test env → default 3.
        assert_eq!(*X_FOLLOWED_BY_MAX, 3);
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test followed_by_max_defaults_to_three`
Expected: FAIL — `cannot find value X_FOLLOWED_BY_MAX in this scope`.

- [ ] **Step 3: Add the constants** inside the `lazy_static! { … }` block in `src/config.rs` (place near the other contract/URL statics)

```rust
    // ---- X (Twitter) hidden-creator verification ----
    pub static ref X_CLIENT_ID: String =
        env::var("X_CLIENT_ID").expect("X_CLIENT_ID must be set");
    /// Confidential (Web App) client → HTTP Basic auth on token exchange.
    /// Empty string ⇒ public (Native) client (PKCE only, no secret).
    pub static ref X_CLIENT_SECRET: String =
        env::var("X_CLIENT_SECRET").unwrap_or_default();
    /// Must EXACTLY match a Callback URI registered in the X app.
    pub static ref X_REDIRECT_URI: String =
        env::var("X_REDIRECT_URI").expect("X_REDIRECT_URI must be set");
    /// Fixed front-end URL to redirect to after a successful callback.
    /// Server-controlled (open-redirect prevention) — never client-supplied.
    pub static ref X_OAUTH_REDIRECT_SUCCESS_URL: String =
        env::var("X_OAUTH_REDIRECT_SUCCESS_URL").expect("X_OAUTH_REDIRECT_SUCCESS_URL must be set");
    /// Fixed front-end URL to redirect to on callback failure.
    pub static ref X_OAUTH_REDIRECT_FAILURE_URL: String =
        env::var("X_OAUTH_REDIRECT_FAILURE_URL").expect("X_OAUTH_REDIRECT_FAILURE_URL must be set");
    /// PKCE state / code_verifier lifetime in Redis (ms). Default 10 min.
    pub static ref X_OAUTH_STATE_TTL_MS: u64 = env::var("X_OAUTH_STATE_TTL_MS")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(600_000);
    /// Pending verification lifetime in Redis (ms). Default 30 min.
    pub static ref X_PENDING_TTL_MS: u64 = env::var("X_PENDING_TTL_MS")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(1_800_000);
    /// Max number of followed-by handles per pending verification. Default 3.
    pub static ref X_FOLLOWED_BY_MAX: usize = env::var("X_FOLLOWED_BY_MAX")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(3);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test followed_by_max_defaults_to_three`
Expected: PASS.

- [ ] **Step 5: Document env vars** — append to `.env.example`

```bash
# X (Twitter) Hidden Creator Verification
# ========================================
# OAuth 2.0 app credentials from the X Developer Console.
X_CLIENT_ID=
# Web App (confidential) → set the secret (Basic auth on token exchange).
# Native App (public)    → leave empty (PKCE only).
X_CLIENT_SECRET=
# Must EXACTLY match a registered Callback URI in the X app (no trailing slash).
X_REDIRECT_URI=https://api.nad.fun/x/oauth/callback
# Fixed front-end redirect targets (server-controlled; open-redirect prevention).
X_OAUTH_REDIRECT_SUCCESS_URL=https://nad.fun/create?x_verified=1
X_OAUTH_REDIRECT_FAILURE_URL=https://nad.fun/create?x_verify_error=1
# PKCE state TTL (ms, default 600000 = 10 min)
X_OAUTH_STATE_TTL_MS=600000
# Pending verification TTL (ms, default 1800000 = 30 min)
X_PENDING_TTL_MS=1800000
# Max followed-by handles per verification (default 3)
X_FOLLOWED_BY_MAX=3
```

- [ ] **Step 6: Commit**

```bash
git add src/config.rs .env.example
git commit -m "feat(x): config env vars for X verification"
```

---

## Task 3: `AppError::Forbidden` (403) + `AppError::Gone` (410)

**Files:**
- Modify: `src/result.rs`
- Test: `src/result.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `AppError::Forbidden(String)` → HTTP 403, `AppError::Gone(String)` → HTTP 410, both bodied `{"error": <msg>}`. Consumed by Tasks 7, 9, 10.

- [ ] **Step 1: Write the failing test**

Add to `src/result.rs`:

```rust
#[cfg(test)]
mod error_status_tests {
    use super::*;
    use axum::response::IntoResponse;
    use axum::http::StatusCode;

    #[test]
    fn forbidden_and_gone_map_to_403_and_410() {
        assert_eq!(
            AppError::Forbidden("creator_mismatch".into()).into_response().status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            AppError::Gone("verification_expired".into()).into_response().status(),
            StatusCode::GONE
        );
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test forbidden_and_gone_map_to_403_and_410`
Expected: FAIL — `no variant named Forbidden`/`Gone`.

- [ ] **Step 3: Add the variants** to `enum AppError` in `src/result.rs`

```rust
    BadRequest(String),
    Forbidden(String),
    Gone(String),
    NotFound(String),
```

- [ ] **Step 4: Add the mappings** in `impl IntoResponse for AppError`, next to the `BadRequest` arm

```rust
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            AppError::Gone(msg) => (StatusCode::GONE, msg),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test forbidden_and_gone_map_to_403_and_410`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/result.rs
git commit -m "feat(error): add Forbidden(403) and Gone(410) variants"
```

---

## Task 4: Public response types + `TokenInfo.x_verification` field

**Files:**
- Create: `src/types/token/x_verification.rs`
- Modify: `src/types/token/mod.rs` (add `pub mod x_verification;`)
- Modify: `src/types/common/info.rs` (add field to `TokenInfo`)
- Modify: 20 `TokenInfo { … }` construction sites (add `x_verification: None`)
- Modify: `src/main.rs` (register 2 schemas)
- Test: `src/types/token/x_verification.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `types::token::x_verification::{TokenXVerification, XFollowedByEntry}`; `TokenInfo.x_verification: Option<TokenXVerification>`. Consumed by Tasks 7, 8, 11.

- [ ] **Step 1: Write the failing test**

Create `src/types/token/x_verification.rs`:

```rust
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Aggregate X-verification signals for one coin. The creator's own X handle
/// and X user-id are NEVER included here.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TokenXVerification {
    pub followers_count: i64,
    pub followed_by: Vec<XFollowedByEntry>,
}

/// A third party that provably follows the creator (public by design).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct XFollowedByEntry {
    pub x_handle: String,
    pub x_image_uri: String,
    pub x_followers_count: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_nested_followed_by() {
        let v = TokenXVerification {
            followers_count: 128_000,
            followed_by: vec![XFollowedByEntry {
                x_handle: "elonmusk".into(),
                x_image_uri: "https://img".into(),
                x_followers_count: 200_000_000,
            }],
        };
        let j = serde_json::to_value(&v).unwrap();
        assert_eq!(j["followers_count"], 128_000);
        assert_eq!(j["followed_by"][0]["x_handle"], "elonmusk");
    }
}
```

- [ ] **Step 2: Declare the module** — add to `src/types/token/mod.rs`

```rust
pub mod x_verification;
```

- [ ] **Step 3: Run test to verify it fails, then passes**

Run: `cargo test --lib serializes_nested_followed_by`
Expected: PASS (this test needs only the new file; run it now to confirm the types compile).

- [ ] **Step 4: Add the field to `TokenInfo`** in `src/types/common/info.rs`

Add the import near the top:

```rust
use crate::types::token::x_verification::TokenXVerification;
```

Add the field as the last field of `struct TokenInfo` (after `version`):

```rust
    pub is_cto: bool,
    pub version: TokenVersion,
    /// Per-coin X verification signals. `None` (⇒ JSON null) when unverified.
    /// NOTE: `TokenInfo` derives `FromRow`; this field is NOT a DB column, so
    /// it must be excluded from row mapping.
    #[serde(default)]
    #[sqlx(default)]
    pub x_verification: Option<TokenXVerification>,
```

(`#[sqlx(default)]` prevents `FromRow` from expecting an `x_verification` column; the field is filled in Rust, not by a query.)

- [ ] **Step 5: Add `x_verification: None` to all 20 construction sites**

At each of these `TokenInfo { … }` literals, add `x_verification: None,` as the last field:

```
src/controllers/token/metadata.rs:~150
src/controllers/token/create.rs:~292
src/controllers/token/order.rs:~503
src/controllers/token/gift_fee.rs:~301
src/controllers/token/mod.rs:~98        (this one gets a REAL value in Task 11 — set None for now)
src/controllers/trend/mod.rs:~271
src/controllers/new_event/mod.rs:~163, ~245, ~321
src/controllers/search/mod.rs:~106
src/controllers/hype/mod.rs:~212, ~335, ~485, ~753, ~1223
src/controllers/chester/mod.rs:~381
src/controllers/dividend/mod.rs:~210
src/controllers/trading/position.rs:~409
src/controllers/trading/swap_history.rs:~195
```

To find every site precisely: `grep -rn "TokenInfo {" src --include=*.rs`. Each literal ends with `version: …,` (or `version: row.version.clone(),`) — insert `x_verification: None,` immediately after it.

- [ ] **Step 6: Register the OpenAPI schemas** — in `src/main.rs`, inside the `schemas( … )` block, right after `types::common::info::TokenCreatedInfo,` (≈ line 195)

```rust
            types::token::x_verification::TokenXVerification,
            types::token::x_verification::XFollowedByEntry,
```

- [ ] **Step 7: Verify the whole crate compiles**

Run: `cargo build 2>&1 | tail -20`
Expected: builds clean. If any `TokenInfo { … }` site was missed, the compiler errors with `missing field x_verification` at that exact file:line — add `x_verification: None,` there and rebuild.

- [ ] **Step 8: Commit**

```bash
git add src/types/token/x_verification.rs src/types/token/mod.rs src/types/common/info.rs src/main.rs src/controllers
git commit -m "feat(x): TokenInfo.x_verification field + public response types"
```

---

## Task 5: X API client (`services/x_oauth`)

**Files:**
- Create: `src/services/x_oauth/mod.rs`
- Create: `src/services/x_oauth/client.rs`
- Modify: `src/services/mod.rs` (add `pub mod x_oauth;`)
- Test: `src/services/x_oauth/client.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  - `generate_pkce() -> (String /*verifier*/, String /*challenge*/)`
  - `build_authorize_url(state: &str, code_challenge: &str) -> String`
  - `async exchange_code(code: &str, code_verifier: &str) -> anyhow::Result<XTokenResponse>` where `XTokenResponse { access_token: String, refresh_token: Option<String>, expires_in: i64 }`
  - `async get_me(access_token: &str) -> anyhow::Result<XUserInfo>` where `XUserInfo { id: String, followers_count: i64 }`
  - `async check_follows_me(access_token: &str, target_handle: &str) -> anyhow::Result<XFollowCheck>` where `XFollowCheck { is_following: bool, x_handle: String, x_image_uri: String, x_followers_count: i64 }`
  - pure parsers `parse_me(&str) -> Option<XUserInfo>`, `parse_follow_check(&str) -> Option<XFollowCheck>`.
- Consumes: `config::{X_CLIENT_ID, X_CLIENT_SECRET, X_REDIRECT_URI}`.

- [ ] **Step 1: Write the failing tests**

Create `src/services/x_oauth/client.rs`:

```rust
use std::time::Duration;

use anyhow::{Context, Result};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use once_cell::sync::Lazy;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::config::{X_CLIENT_ID, X_CLIENT_SECRET, X_REDIRECT_URI};

const AUTHORIZE_URL: &str = "https://x.com/i/oauth2/authorize";
const TOKEN_URL: &str = "https://api.x.com/2/oauth2/token";
const API_BASE: &str = "https://api.x.com/2";
// Space-separated scopes, pre-encoded as %20 to avoid +/space ambiguity.
const SCOPES: &str = "users.read%20tweet.read%20offline.access";

static HTTP: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .expect("failed to build reqwest client")
});

#[derive(Debug, Clone, Deserialize)]
pub struct XTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_in: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct XUserInfo {
    pub id: String,
    pub followers_count: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct XFollowCheck {
    pub is_following: bool,
    pub x_handle: String,
    pub x_image_uri: String,
    pub x_followers_count: i64,
}

/// PKCE: random verifier (64 url-safe chars, within the 43-128 range) and its
/// S256 challenge = base64url(sha256(verifier)).
pub fn generate_pkce() -> (String, String) {
    let bytes: [u8; 48] = rand::random();
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

pub fn build_authorize_url(state: &str, code_challenge: &str) -> String {
    format!(
        "{AUTHORIZE_URL}?response_type=code&client_id={cid}&redirect_uri={ruri}\
&scope={scope}&state={state}&code_challenge={chal}&code_challenge_method=S256",
        cid = urlencoding::encode(&X_CLIENT_ID),
        ruri = urlencoding::encode(&X_REDIRECT_URI),
        scope = SCOPES,
        state = urlencoding::encode(state),
        chal = code_challenge, // already url-safe (no pad)
    )
}

/// Larger avatar: X returns a `_normal` (48px) image; `_400x400` is higher-res.
fn upscale_avatar(url: &str) -> String {
    url.replace("_normal", "_400x400")
}

pub(crate) fn parse_me(body: &str) -> Option<XUserInfo> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let data = v.get("data")?;
    let id = data.get("id")?.as_str()?.to_string();
    let followers_count = data
        .get("public_metrics")
        .and_then(|m| m.get("followers_count"))
        .and_then(|n| n.as_i64())
        .unwrap_or(0);
    Some(XUserInfo { id, followers_count })
}

pub(crate) fn parse_follow_check(body: &str) -> Option<XFollowCheck> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let data = v.get("data")?;
    let x_handle = data.get("username")?.as_str()?.to_string();
    let is_following = data
        .get("connection_status")
        .and_then(|c| c.as_array())
        .map(|arr| arr.iter().any(|s| s.as_str() == Some("followed_by")))
        .unwrap_or(false);
    let x_image_uri = data
        .get("profile_image_url")
        .and_then(|s| s.as_str())
        .map(upscale_avatar)
        .unwrap_or_default();
    let x_followers_count = data
        .get("public_metrics")
        .and_then(|m| m.get("followers_count"))
        .and_then(|n| n.as_i64())
        .unwrap_or(0);
    Some(XFollowCheck { is_following, x_handle, x_image_uri, x_followers_count })
}

pub async fn exchange_code(code: &str, code_verifier: &str) -> Result<XTokenResponse> {
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", X_REDIRECT_URI.as_str()),
        ("code_verifier", code_verifier),
        ("client_id", X_CLIENT_ID.as_str()),
    ];
    // Public client relies on client_id in the body; confidential client ALSO
    // authenticates with HTTP Basic (client_id:client_secret).
    let mut req = HTTP.post(TOKEN_URL).form(&form);
    if !X_CLIENT_SECRET.is_empty() {
        req = req.basic_auth(X_CLIENT_ID.as_str(), Some(X_CLIENT_SECRET.as_str()));
    }
    let resp = req.send().await.context("x token request failed")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("x token endpoint status {}: {}", status, body);
    }
    serde_json::from_str(&body).context("decode x token response")
}

pub async fn get_me(access_token: &str) -> Result<XUserInfo> {
    let url = format!("{API_BASE}/users/me?user.fields=public_metrics");
    let resp = HTTP
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .context("x get_me request failed")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("x get_me status {}: {}", status, body);
    }
    parse_me(&body).context("parse x get_me response")
}

pub async fn check_follows_me(access_token: &str, target_handle: &str) -> Result<XFollowCheck> {
    let url = format!(
        "{API_BASE}/users/by/username/{}?user.fields=connection_status,profile_image_url,public_metrics",
        urlencoding::encode(target_handle)
    );
    let resp = HTTP
        .get(&url)
        .bearer_auth(access_token)
        .send()
        .await
        .context("x follow-check request failed")?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("x follow-check status {}: {}", status, body);
    }
    parse_follow_check(&body).context("parse x follow-check response")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_s256_of_verifier() {
        let (verifier, challenge) = generate_pkce();
        assert!(verifier.len() >= 43 && verifier.len() <= 128);
        let expected = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        assert_eq!(challenge, expected);
    }

    #[test]
    fn parse_me_extracts_id_and_followers() {
        let body = r#"{"data":{"id":"123","username":"creator",
            "public_metrics":{"followers_count":128000,"following_count":10}}}"#;
        let me = parse_me(body).unwrap();
        assert_eq!(me, XUserInfo { id: "123".into(), followers_count: 128000 });
    }

    #[test]
    fn parse_follow_check_detects_followed_by() {
        let body = r#"{"data":{"connection_status":["followed_by","following"],
            "id":"1","username":"builnad",
            "profile_image_url":"https://pbs.twimg.com/x_normal.jpg",
            "public_metrics":{"followers_count":5000}}}"#;
        let c = parse_follow_check(body).unwrap();
        assert!(c.is_following);
        assert_eq!(c.x_handle, "builnad");
        assert_eq!(c.x_image_uri, "https://pbs.twimg.com/x_400x400.jpg");
        assert_eq!(c.x_followers_count, 5000);
    }

    #[test]
    fn parse_follow_check_false_when_not_followed() {
        let body = r#"{"data":{"connection_status":["following"],"id":"1",
            "username":"b","profile_image_url":"u_normal.jpg","public_metrics":{"followers_count":1}}}"#;
        assert!(!parse_follow_check(body).unwrap().is_following);
    }
}
```

- [ ] **Step 2: Create the module file** `src/services/x_oauth/mod.rs`

```rust
pub mod client;
```

- [ ] **Step 3: Declare the module** — add to `src/services/mod.rs`

```rust
pub mod x_oauth;
```

- [ ] **Step 4: Run the tests**

Run: `cargo test --lib x_oauth::client`
Expected: FAIL first (before Steps 1-3 land), then PASS for all 4 tests once the file compiles.

- [ ] **Step 5: Commit**

```bash
git add src/services/x_oauth src/services/mod.rs
git commit -m "feat(x): X OAuth2 PKCE API client"
```

---

## Task 6: Internal DTOs + pure helpers (`types/x_verification`)

**Files:**
- Create: `src/types/x_verification/mod.rs`
- Modify: `src/types/mod.rs` (add `pub mod x_verification;`)
- Test: `src/types/x_verification/mod.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces:
  - Redis structs: `XOAuthState { account_id, code_verifier }`, `XPending { x_user_id, access_token, followers_count, followed_by: Vec<XFollowedByEntry> }`.
  - DTOs: `OAuthLoginResponse { authorize_url }`, `OAuthCallbackQuery { code, state, error }`, `FollowedByRequest { handle }`, `FollowedByResponse { is_following, entry: Option<XFollowedByEntry> }`, `PendingResponse { followers_count: Option<i64>, followed_by: Vec<XFollowedByEntry> }`, `FinalizeRequest { token_id, creator, name, symbol, metadata_uri, salt, version }`, `FinalizeResponse { ok }`.
  - Pure helpers: `validate_handle(&str) -> bool`, `append_followed_by(&mut Vec<XFollowedByEntry>, XFollowedByEntry, max: usize) -> Result<(), &'static str>`.
- Consumes: `types::token::x_verification::XFollowedByEntry`, `types::common::info::TokenVersion`.

- [ ] **Step 1: Write the failing tests + types**

Create `src/types/x_verification/mod.rs`:

```rust
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::TokenVersion;
use crate::types::token::x_verification::XFollowedByEntry;

// ---- Redis-serialized (never returned to clients directly) ----

/// `x_oauth:state:{state}` payload. Consumed once via GETDEL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XOAuthState {
    pub account_id: String,
    pub code_verifier: String,
}

/// `x_pending:{account_id}` payload. Holds the creator's short-lived token +
/// collected signals until finalize.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XPending {
    pub x_user_id: String,
    pub access_token: String,
    pub followers_count: i64,
    #[serde(default)]
    pub followed_by: Vec<XFollowedByEntry>,
}

// ---- Request / response DTOs ----

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OAuthLoginResponse {
    pub authorize_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct OAuthCallbackQuery {
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct FollowedByRequest {
    pub handle: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FollowedByResponse {
    pub is_following: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry: Option<XFollowedByEntry>,
}

/// Public pending view — the creator's own handle / user-id are intentionally absent.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PendingResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub followers_count: Option<i64>,
    pub followed_by: Vec<XFollowedByEntry>,
}

fn default_version() -> TokenVersion {
    TokenVersion::V1
}

/// The exact salt-mining params are re-sent so the server can recompute the
/// CREATE2 address (see §7.1). `name`/`symbol`/`metadata_uri` are accepted for
/// request parity but do NOT affect the CREATE2 address and are not persisted.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct FinalizeRequest {
    pub token_id: String,
    pub creator: String,
    pub name: String,
    pub symbol: String,
    pub metadata_uri: String,
    pub salt: String,
    #[serde(default = "default_version")]
    pub version: TokenVersion,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct FinalizeResponse {
    pub ok: bool,
}

// ---- Pure helpers ----

/// X handle rule: 1-15 chars of [A-Za-z0-9_].
pub fn validate_handle(handle: &str) -> bool {
    let len = handle.chars().count();
    (1..=15).contains(&len)
        && handle.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Append with case-insensitive de-dup by handle; enforce `max`.
/// Err("max_followed_by_reached") when already at capacity with a new handle.
pub fn append_followed_by(
    list: &mut Vec<XFollowedByEntry>,
    entry: XFollowedByEntry,
    max: usize,
) -> Result<(), &'static str> {
    if let Some(existing) = list
        .iter_mut()
        .find(|e| e.x_handle.eq_ignore_ascii_case(&entry.x_handle))
    {
        *existing = entry; // refresh snapshot
        return Ok(());
    }
    if list.len() >= max {
        return Err("max_followed_by_reached");
    }
    list.push(entry);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(h: &str) -> XFollowedByEntry {
        XFollowedByEntry { x_handle: h.into(), x_image_uri: "u".into(), x_followers_count: 1 }
    }

    #[test]
    fn handle_validation() {
        assert!(validate_handle("elonmusk"));
        assert!(validate_handle("a_1"));
        assert!(!validate_handle(""));
        assert!(!validate_handle("waytoolonghandle16"));
        assert!(!validate_handle("has space"));
        assert!(!validate_handle("bad-dash"));
    }

    #[test]
    fn append_enforces_max_and_dedup() {
        let mut list = vec![];
        assert!(append_followed_by(&mut list, e("a"), 3).is_ok());
        assert!(append_followed_by(&mut list, e("b"), 3).is_ok());
        // case-insensitive dedup replaces, does not grow
        assert!(append_followed_by(&mut list, e("A"), 3).is_ok());
        assert_eq!(list.len(), 2);
        assert!(append_followed_by(&mut list, e("c"), 3).is_ok());
        // now full → new handle rejected
        assert_eq!(append_followed_by(&mut list, e("d"), 3), Err("max_followed_by_reached"));
        assert_eq!(list.len(), 3);
    }
}
```

- [ ] **Step 2: Declare the module** — add to `src/types/mod.rs`

```rust
pub mod x_verification;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --lib x_verification::` (types layer) — specifically `handle_validation`, `append_enforces_max_and_dedup`.
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src/types/x_verification src/types/mod.rs
git commit -m "feat(x): internal DTOs + pure handle/append helpers"
```

---

## Task 7: Redis pending + finalize ownership checks + orchestration service

**Files:**
- Modify: `src/services/token/salt.rs` (expose CREATE2)
- Modify: `src/db/redis/mod.rs` (state + pending methods)
- Create: `src/services/x_verification/mod.rs` (orchestration + `creator_matches` / `address_matches`)
- Modify: `src/services/mod.rs` (add `pub mod x_verification;`)
- Test: `src/services/x_verification/mod.rs` (`#[cfg(test)]`) — the security-critical finalize checks

**Interfaces:**
- Produces:
  - `SaltService::compute_token_address(version: TokenVersion, salt: B256) -> Result<Address, AppError>`.
  - `RedisDatabase::{set_x_oauth_state, get_and_delete_x_oauth_state, set_x_pending, get_x_pending, delete_x_pending}`.
  - `services::x_verification::{creator_matches, address_matches}` (pure) and `XVerificationService` (orchestration).
- Consumes: Tasks 3, 5, 6; `config::{X_OAUTH_STATE_TTL_MS, X_PENDING_TTL_MS, X_FOLLOWED_BY_MAX}`; `utils::valid_account_id`.

- [ ] **Step 1: Write the failing security tests**

Create `src/services/x_verification/mod.rs` (tests first, at the bottom of the file you will build in Steps 4-6):

```rust
#[cfg(test)]
mod finalize_checks {
    use super::{address_matches, creator_matches};
    use crate::result::AppError;
    use crate::services::token::salt::SaltService;
    use alloy::primitives::{Address, B256};
    use std::str::FromStr;

    // Known-good deployer/impl (same fixtures as salt.rs tests). We derive a
    // REAL address from a salt with compute_create2_address so the test is
    // independent of env-loaded config.
    fn fixture_address() -> Address {
        let deployer = Address::from_str("0x5FbDB2315678afecb367f032d93F642f64180aa3").unwrap();
        let implementation = Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap();
        SaltService::compute_create2_address(deployer, implementation, B256::from([7u8; 32]))
    }

    #[test]
    fn address_match_accepts_exact_and_rejects_other() {
        let addr = fixture_address();
        // exact (any case) passes
        assert!(address_matches(addr, &addr.to_string()).is_ok());
        // a different token_id is rejected with 403 creator_mismatch (IDOR guard)
        let other = "0x000000000000000000000000000000000000dEaD";
        let err = address_matches(addr, other).unwrap_err();
        assert!(matches!(err, AppError::Forbidden(ref m) if m == "creator_mismatch"));
    }

    #[test]
    fn creator_match_is_checksum_case_insensitive() {
        let a = "0x742d35Cc6634C0532925a3b844Bc9e7595f70143";
        // same address, different casing → OK (checksum compare, never LOWER())
        assert!(creator_matches(a, &a.to_lowercase()).is_ok());
        // different address → 403
        let b = "0x0000000000000000000000000000000000000001";
        let err = creator_matches(a, b).unwrap_err();
        assert!(matches!(err, AppError::Forbidden(ref m) if m == "creator_mismatch"));
    }
}
```

- [ ] **Step 2: Expose the CREATE2 primitive** in `src/services/token/salt.rs`

Change the visibility of the existing method (line ≈233):

```rust
    pub(crate) fn compute_create2_address(deployer: Address, implementation: Address, salt: B256) -> Address {
```

Add a public wrapper inside `impl SaltService` (after `mine_salt`):

```rust
    /// Recompute the deterministic CREATE2 token address for a known salt +
    /// version. Used by finalize to verify a client-supplied `token_id` (§7.1).
    pub fn compute_token_address(
        version: TokenVersion,
        salt: B256,
    ) -> Result<Address, AppError> {
        let config = MiningConfig::load(version)?;
        Ok(Self::compute_create2_address(config.deployer, config.implementation, salt))
    }
```

(`TokenVersion`, `AppError`, `MiningConfig`, `Address`, `B256` are already imported in this file.)

- [ ] **Step 3: Add Redis methods** — new `impl RedisDatabase` block at the end of `src/db/redis/mod.rs`

```rust
// X (Twitter) verification: PKCE state + pending signals
impl RedisDatabase {
    pub async fn set_x_oauth_state(
        &self,
        state: &str,
        payload: &crate::types::x_verification::XOAuthState,
        ttl_ms: u64,
    ) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let key = with_prefix(format!("x_oauth:state:{}", state));
        let json = serde_json::to_string(payload)?;
        measure_redis!(
            "redis.set_x_oauth_state",
            conn.pset_ex::<String, String, ()>(key, json, ttl_ms)
        )?;
        Ok(())
    }

    /// Atomically fetch + delete the PKCE state (one-time consume; anti-replay).
    pub async fn get_and_delete_x_oauth_state(
        &self,
        state: &str,
    ) -> Result<crate::types::x_verification::XOAuthState> {
        let mut conn = self.conn.as_ref().clone();
        let key = with_prefix(format!("x_oauth:state:{}", state));
        let json: Option<String> = measure_redis!(
            "redis.get_and_delete_x_oauth_state",
            redis::cmd("GETDEL").arg(&key).query_async(&mut conn)
        )?;
        match json {
            Some(j) => Ok(serde_json::from_str(&j)?),
            None => Err(anyhow::anyhow!("state not found or already used")),
        }
    }

    pub async fn set_x_pending(
        &self,
        account_id: &str,
        pending: &crate::types::x_verification::XPending,
        ttl_ms: u64,
    ) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let key = with_prefix(format!("x_pending:{}", account_id));
        let json = serde_json::to_string(pending)?;
        measure_redis!(
            "redis.set_x_pending",
            conn.pset_ex::<String, String, ()>(key, json, ttl_ms)
        )?;
        Ok(())
    }

    pub async fn get_x_pending(
        &self,
        account_id: &str,
    ) -> Result<Option<crate::types::x_verification::XPending>> {
        let mut conn = self.conn.as_ref().clone();
        let key = with_prefix(format!("x_pending:{}", account_id));
        let json: Option<String> =
            measure_redis!("redis.get_x_pending", conn.get::<_, Option<String>>(key))?;
        match json {
            Some(j) => Ok(Some(serde_json::from_str(&j)?)),
            None => Ok(None),
        }
    }

    pub async fn delete_x_pending(&self, account_id: &str) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let key = with_prefix(format!("x_pending:{}", account_id));
        measure_redis!("redis.delete_x_pending", conn.del::<String, ()>(key))?;
        Ok(())
    }
}
```

- [ ] **Step 4: Write the pure finalize checks** at the top of `src/services/x_verification/mod.rs`

```rust
use std::str::FromStr;
use std::sync::Arc;

use alloy::primitives::{Address, B256};
use tracing::error;

use crate::config::{X_FOLLOWED_BY_MAX, X_PENDING_TTL_MS};
use crate::controllers::x_verification::XVerificationController;
use crate::db::{postgres::PostgresDatabase, redis::RedisDatabase};
use crate::result::AppError;
use crate::services::token::salt::SaltService;
use crate::services::x_oauth::client;
use crate::types::token::x_verification::XFollowedByEntry;
use crate::types::x_verification::{
    FinalizeRequest, FollowedByResponse, OAuthLoginResponse, PendingResponse, XOAuthState, XPending,
    append_followed_by,
};
use crate::utils::valid_account_id;

/// creator (from request) must equal the authenticated session wallet.
/// EIP-55 checksum comparison — never LOWER().
pub fn creator_matches(creator: &str, session_address: &str) -> Result<(), AppError> {
    let creator_cs = valid_account_id(creator)
        .ok_or_else(|| AppError::BadRequest("invalid creator address".into()))?;
    let session_cs = valid_account_id(session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    if creator_cs != session_cs {
        return Err(AppError::Forbidden("creator_mismatch".into()));
    }
    Ok(())
}

/// The recomputed CREATE2 address must equal the client-supplied token_id.
/// Address byte-equality is checksum-agnostic (equivalent to EIP-55 compare).
pub fn address_matches(expected: Address, token_id: &str) -> Result<(), AppError> {
    let claimed = Address::from_str(token_id)
        .map_err(|_| AppError::BadRequest("invalid token_id".into()))?;
    if expected != claimed {
        return Err(AppError::Forbidden("creator_mismatch".into()));
    }
    Ok(())
}
```

- [ ] **Step 5: Add the orchestration service** (same file, after the pure fns)

```rust
pub struct XVerificationService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl XVerificationService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    /// Start OAuth: make PKCE, stash state→(account_id,verifier), return authorize URL.
    pub async fn start_login(&self, account_id: &str) -> Result<OAuthLoginResponse, AppError> {
        let (verifier, challenge) = client::generate_pkce();
        let state = {
            let bytes: [u8; 24] = rand::random();
            hex::encode(bytes)
        };
        let payload = XOAuthState {
            account_id: account_id.to_string(),
            code_verifier: verifier,
        };
        self.redis
            .set_x_oauth_state(&state, &payload, *crate::config::X_OAUTH_STATE_TTL_MS)
            .await
            .map_err(|e| AppError::InternalError(format!("save state: {e}")))?;
        Ok(OAuthLoginResponse { authorize_url: client::build_authorize_url(&state, &challenge) })
    }

    /// Handle the OAuth callback: consume state, exchange code, fetch followers,
    /// stash pending. Returns the account_id the pending was stored under.
    pub async fn complete_callback(&self, code: &str, state: &str) -> Result<String, AppError> {
        let st = self
            .redis
            .get_and_delete_x_oauth_state(state)
            .await
            .map_err(|_| AppError::Gone("state_expired".into()))?;
        let token = client::exchange_code(code, &st.code_verifier)
            .await
            .map_err(|e| AppError::InternalError(format!("code exchange: {e}")))?;
        let me = client::get_me(&token.access_token)
            .await
            .map_err(|e| AppError::InternalError(format!("get_me: {e}")))?;
        let pending = XPending {
            x_user_id: me.id,
            access_token: token.access_token,
            followers_count: me.followers_count,
            followed_by: vec![],
        };
        self.redis
            .set_x_pending(&st.account_id, &pending, *X_PENDING_TTL_MS)
            .await
            .map_err(|e| AppError::InternalError(format!("save pending: {e}")))?;
        Ok(st.account_id)
    }

    pub async fn add_followed_by(
        &self,
        account_id: &str,
        handle: &str,
    ) -> Result<FollowedByResponse, AppError> {
        let mut pending = self
            .redis
            .get_x_pending(account_id)
            .await
            .map_err(|e| AppError::InternalError(format!("read pending: {e}")))?
            .ok_or_else(|| AppError::Gone("verification_expired".into()))?;

        // Cap BEFORE spending an X API call.
        if pending.followed_by.len() >= *X_FOLLOWED_BY_MAX {
            return Err(AppError::BadRequest("max_followed_by_reached".into()));
        }

        let check = client::check_follows_me(&pending.access_token, handle)
            .await
            .map_err(|e| AppError::InternalError(format!("follow check: {e}")))?;

        if !check.is_following {
            return Ok(FollowedByResponse { is_following: false, entry: None });
        }

        let entry = XFollowedByEntry {
            x_handle: check.x_handle,
            x_image_uri: check.x_image_uri,
            x_followers_count: check.x_followers_count,
        };
        append_followed_by(&mut pending.followed_by, entry.clone(), *X_FOLLOWED_BY_MAX)
            .map_err(|m| AppError::BadRequest(m.to_string()))?;
        self.redis
            .set_x_pending(account_id, &pending, *X_PENDING_TTL_MS)
            .await
            .map_err(|e| AppError::InternalError(format!("save pending: {e}")))?;
        Ok(FollowedByResponse { is_following: true, entry: Some(entry) })
    }

    pub async fn remove_followed_by(
        &self,
        account_id: &str,
        handle: &str,
    ) -> Result<PendingResponse, AppError> {
        let mut pending = self
            .redis
            .get_x_pending(account_id)
            .await
            .map_err(|e| AppError::InternalError(format!("read pending: {e}")))?
            .ok_or_else(|| AppError::Gone("verification_expired".into()))?;
        pending
            .followed_by
            .retain(|e| !e.x_handle.eq_ignore_ascii_case(handle));
        self.redis
            .set_x_pending(account_id, &pending, *X_PENDING_TTL_MS)
            .await
            .map_err(|e| AppError::InternalError(format!("save pending: {e}")))?;
        Ok(PendingResponse {
            followers_count: Some(pending.followers_count),
            followed_by: pending.followed_by,
        })
    }

    /// Public pending view — omits the creator's own handle / user-id.
    pub async fn get_pending(&self, account_id: &str) -> Result<PendingResponse, AppError> {
        match self
            .redis
            .get_x_pending(account_id)
            .await
            .map_err(|e| AppError::InternalError(format!("read pending: {e}")))?
        {
            Some(p) => Ok(PendingResponse {
                followers_count: Some(p.followers_count),
                followed_by: p.followed_by,
            }),
            None => Ok(PendingResponse { followers_count: None, followed_by: vec![] }),
        }
    }

    /// Verify ownership (§7.1) then persist to Postgres and clear pending.
    pub async fn finalize(
        &self,
        req: &FinalizeRequest,
        session_address: &str,
    ) -> Result<(), AppError> {
        // 1. creator == session wallet
        creator_matches(&req.creator, session_address)?;
        // 2. recompute CREATE2 address and compare to token_id
        let salt = B256::from_str(&req.salt)
            .map_err(|_| AppError::BadRequest("invalid salt".into()))?;
        let expected = SaltService::compute_token_address(req.version.clone(), salt)?;
        address_matches(expected, &req.token_id)?;

        // 3. pending must still exist
        let pending = self
            .redis
            .get_x_pending(session_address)
            .await
            .map_err(|e| AppError::InternalError(format!("read pending: {e}")))?
            .ok_or_else(|| AppError::Gone("verification_expired".into()))?;

        // 4. persist (idempotent upsert)
        let controller = XVerificationController::new(self.postgres.clone());
        controller
            .finalize(
                &req.token_id,
                session_address,
                &pending.x_user_id,
                pending.followers_count,
                &pending.followed_by,
            )
            .await
            .map_err(|e| {
                error!("finalize persist failed: {e}");
                AppError::InternalError("finalize failed".into())
            })?;

        // 5. clear pending (token is now bound; token no longer re-usable)
        let _ = self.redis.delete_x_pending(session_address).await;
        Ok(())
    }
}
```

- [ ] **Step 6: Declare the module** — add to `src/services/mod.rs`

```rust
pub mod x_verification;
```

- [ ] **Step 7: Run the security tests**

Run: `cargo test --lib finalize_checks`
Expected: PASS — `address_match_accepts_exact_and_rejects_other`, `creator_match_is_checksum_case_insensitive`. (Task 8 must land for the module to fully compile, since `finalize` references `XVerificationController`. If compiling this task alone, temporarily stub the controller import; otherwise implement Task 8 first and run this test after. Recommended: do Task 8 before running.)

- [ ] **Step 8: Commit**

```bash
git add src/services/token/salt.rs src/db/redis/mod.rs src/services/x_verification src/services/mod.rs
git commit -m "feat(x): redis pending + finalize ownership checks + orchestration"
```

---

## Task 8: Postgres controller (finalize persistence)

**Files:**
- Create: `src/controllers/x_verification/mod.rs`
- Modify: `src/controllers/mod.rs` (add `pub mod x_verification;`)
- Test: `src/controllers/x_verification/mod.rs` (`#[sqlx::test]`)

**Interfaces:**
- Produces: `XVerificationController::new(Arc<PostgresDatabase>)`, `async finalize(token_id, account_id, x_user_id, followers_count, followed_by: &[XFollowedByEntry]) -> anyhow::Result<()>`.
- Consumes: Task 1 tables; `types::token::x_verification::XFollowedByEntry`.

- [ ] **Step 1: Write the failing test**

Create `src/controllers/x_verification/mod.rs`:

```rust
use std::sync::Arc;

use anyhow::{Result, anyhow};

use crate::{db::postgres::PostgresDatabase, types::token::x_verification::XFollowedByEntry};

pub struct XVerificationController {
    db: Arc<PostgresDatabase>,
}

impl XVerificationController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    /// Idempotent upsert of a verification + its followed_by rows (transaction).
    pub async fn finalize(
        &self,
        token_id: &str,
        account_id: &str,
        x_user_id: &str,
        followers_count: i64,
        followed_by: &[XFollowedByEntry],
    ) -> Result<()> {
        let mut tx = self
            .db
            .get_write_pool()
            .begin()
            .await
            .map_err(|e| anyhow!("begin tx: {e}"))?;

        sqlx::query(
            r#"
            INSERT INTO token_x_verification (token_id, account_id, x_user_id, followers_count)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (token_id) DO UPDATE
              SET account_id = EXCLUDED.account_id,
                  x_user_id = EXCLUDED.x_user_id,
                  followers_count = EXCLUDED.followers_count,
                  verified_at = NOW()
            "#,
        )
        .bind(token_id)
        .bind(account_id)
        .bind(x_user_id)
        .bind(followers_count)
        .execute(&mut *tx)
        .await
        .map_err(|e| anyhow!("upsert verification: {e}"))?;

        sqlx::query("DELETE FROM token_x_followed_by WHERE token_id = $1")
            .bind(token_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| anyhow!("clear followed_by: {e}"))?;

        for fb in followed_by {
            sqlx::query(
                r#"
                INSERT INTO token_x_followed_by
                    (token_id, x_handle, x_image_uri, x_followers_count)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (token_id, x_handle) DO NOTHING
                "#,
            )
            .bind(token_id)
            .bind(&fb.x_handle)
            .bind(&fb.x_image_uri)
            .bind(fb.x_followers_count)
            .execute(&mut *tx)
            .await
            .map_err(|e| anyhow!("insert followed_by: {e}"))?;
        }

        tx.commit().await.map_err(|e| anyhow!("commit: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::token::x_verification::XFollowedByEntry;
    use sqlx::PgPool;

    const TOKEN: &str = "0x000000000000000000000000000000000000B143";
    const ACCOUNT: &str = "0x000000000000000000000000000000000000Aa01";

    fn ctrl(pool: PgPool) -> XVerificationController {
        XVerificationController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn finalize_persists_verification_and_followers(pool: PgPool) {
        let fb = vec![XFollowedByEntry {
            x_handle: "elonmusk".into(),
            x_image_uri: "https://img".into(),
            x_followers_count: 200_000_000,
        }];
        ctrl(pool.clone())
            .finalize(TOKEN, ACCOUNT, "999", 128_000, &fb)
            .await
            .unwrap();

        let (fc,): (i64,) =
            sqlx::query_as("SELECT followers_count FROM token_x_verification WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(fc, 128_000);

        let (h,): (String,) =
            sqlx::query_as("SELECT x_handle FROM token_x_followed_by WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(h, "elonmusk");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn finalize_is_idempotent(pool: PgPool) {
        let fb = vec![XFollowedByEntry {
            x_handle: "a".into(), x_image_uri: "u".into(), x_followers_count: 1,
        }];
        let c = ctrl(pool.clone());
        c.finalize(TOKEN, ACCOUNT, "1", 10, &fb).await.unwrap();
        c.finalize(TOKEN, ACCOUNT, "1", 20, &fb).await.unwrap(); // re-run
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM token_x_followed_by WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(n, 1, "re-finalize must not duplicate followed_by rows");
        let (fc,): (i64,) =
            sqlx::query_as("SELECT followers_count FROM token_x_verification WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(fc, 20, "followers_count refreshed on re-finalize");
    }
}
```

- [ ] **Step 2: Declare the module** — add to `src/controllers/mod.rs`

```rust
pub mod x_verification;
```

- [ ] **Step 3: Run the tests**

Run: `cargo test --lib x_verification::tests::finalize`
Expected: FAIL first (module missing), then PASS both once compiled (requires local Postgres for `#[sqlx::test]`).

- [ ] **Step 4: Remove the throwaway migrations smoke test** from Task 1 Step 7 (these real tests supersede it).

- [ ] **Step 5: Commit**

```bash
git add src/controllers/x_verification src/controllers/mod.rs
git commit -m "feat(x): postgres finalize persistence + tests"
```

---

## Task 9: Router — 6 endpoints (auth + public callback)

**Files:**
- Create: `src/router/x_verification/path.rs`
- Create: `src/router/x_verification/handler.rs`
- Create: `src/router/x_verification/mod.rs`
- Modify: `src/services/rate_limiter.rs` (2 consts)
- Modify: `src/router/mod.rs` (declare module)
- Modify: `src/middleware.rs` (exclude callback from `api_key_gate`)
- Modify: `src/main.rs` (import, `.merge`, OpenAPI `paths`)

**Interfaces:**
- Produces routes: `POST /x/oauth/login`, `GET /x/oauth/callback`, `POST /x/followed-by`, `DELETE /x/followed-by/:handle`, `GET /x/verification/pending`, `POST /x/verification/finalize`.
- Consumes: Tasks 6, 7, 8; `middleware::authenticate_user`; `services::rate_limiter::check_and_increment`.

- [ ] **Step 1: Add rate-limit constants** to `src/services/rate_limiter.rs`

```rust
/// X OAuth login attempts per account per minute.
pub const X_OAUTH_LOGIN_RATE_LIMIT: u64 = 3;
/// X followed-by checks per account per minute.
pub const X_FOLLOWED_BY_RATE_LIMIT: u64 = 5;
```

- [ ] **Step 2: Write `path.rs`**

Create `src/router/x_verification/path.rs`:

```rust
#[derive(Debug)]
pub enum XVerificationPath {
    OauthLogin,
    OauthCallback,
    FollowedBy,
    FollowedByDelete,
    Pending,
    Finalize,
}

impl XVerificationPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            XVerificationPath::OauthLogin => "/x/oauth/login",
            XVerificationPath::OauthCallback => "/x/oauth/callback",
            XVerificationPath::FollowedBy => "/x/followed-by",
            XVerificationPath::FollowedByDelete => "/x/followed-by/:handle",
            XVerificationPath::Pending => "/x/verification/pending",
            XVerificationPath::Finalize => "/x/verification/finalize",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            XVerificationPath::OauthLogin => "/x/oauth/login",
            XVerificationPath::OauthCallback => "/x/oauth/callback",
            XVerificationPath::FollowedBy => "/x/followed-by",
            XVerificationPath::FollowedByDelete => "/x/followed-by/{handle}",
            XVerificationPath::Pending => "/x/verification/pending",
            XVerificationPath::Finalize => "/x/verification/finalize",
        }
    }
}
```

- [ ] **Step 3: Write `handler.rs`**

Create `src/router/x_verification/handler.rs`:

```rust
use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    response::Redirect,
};
use tracing::instrument;

use super::path::XVerificationPath;
use crate::{
    config::{X_OAUTH_REDIRECT_FAILURE_URL, X_OAUTH_REDIRECT_SUCCESS_URL},
    result::{AppError, AppJsonResult},
    services::rate_limiter::{
        RateLimitResult, X_FOLLOWED_BY_RATE_LIMIT, X_OAUTH_LOGIN_RATE_LIMIT, check_and_increment,
    },
    services::x_verification::XVerificationService,
    state::AppState,
    types::x_verification::{
        FinalizeRequest, FinalizeResponse, FollowedByRequest, FollowedByResponse,
        OAuthCallbackQuery, OAuthLoginResponse, PendingResponse, validate_handle,
    },
    utils::valid_account_id,
};

fn service(state: &AppState) -> XVerificationService {
    XVerificationService::new(state.postgres.clone(), state.redis.clone())
}

/// Append a fixed error code to the (server-controlled) failure URL.
fn failure_redirect(code: &str) -> Redirect {
    let sep = if X_OAUTH_REDIRECT_FAILURE_URL.contains('?') { '&' } else { '?' };
    Redirect::to(&format!("{}{}x_verify_error={}", *X_OAUTH_REDIRECT_FAILURE_URL, sep, code))
}

/// POST /x/oauth/login — start PKCE OAuth. Returns the X authorize URL.
#[utoipa::path(
    post, path = XVerificationPath::OauthLogin.docs_str(),
    responses((status = 200, body = OAuthLoginResponse)),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address))]
pub async fn oauth_login(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<OAuthLoginResponse> {
    rate_limit(&state, &format!("x_login:{session_address}"), X_OAUTH_LOGIN_RATE_LIMIT).await?;
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    let resp = service(&state).start_login(&account_id).await?;
    Ok(Json(resp))
}

/// GET /x/oauth/callback — PUBLIC (X redirects here). Always redirects the
/// browser to a FIXED server env URL (never client-supplied).
#[utoipa::path(
    get, path = XVerificationPath::OauthCallback.docs_str(),
    responses((status = 302, description = "Redirect to fixed front-end URL")),
    tag = "XVerification"
)]
#[instrument(skip(state))]
pub async fn oauth_callback(
    State(state): State<AppState>,
    Query(q): Query<OAuthCallbackQuery>,
) -> Redirect {
    if q.error.is_some() {
        return failure_redirect("denied");
    }
    let (code, st) = match (q.code, q.state) {
        (Some(c), Some(s)) if !c.is_empty() && !s.is_empty() => (c, s),
        _ => return failure_redirect("bad_request"),
    };
    match service(&state).complete_callback(&code, &st).await {
        Ok(_) => Redirect::to(&X_OAUTH_REDIRECT_SUCCESS_URL),
        Err(AppError::Gone(_)) => failure_redirect("state_expired"),
        Err(_) => failure_redirect("x_error"),
    }
}

/// POST /x/followed-by — check + record that a handle follows the creator.
#[utoipa::path(
    post, path = XVerificationPath::FollowedBy.docs_str(),
    request_body = FollowedByRequest,
    responses((status = 200, body = FollowedByResponse)),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address))]
pub async fn add_followed_by(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<FollowedByRequest>,
) -> AppJsonResult<FollowedByResponse> {
    let handle = payload.handle.trim().trim_start_matches('@').to_string();
    if !validate_handle(&handle) {
        return Err(AppError::BadRequest("invalid_handle".into()));
    }
    rate_limit(&state, &format!("x_followed_by:{session_address}"), X_FOLLOWED_BY_RATE_LIMIT).await?;
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    let resp = service(&state).add_followed_by(&account_id, &handle).await?;
    Ok(Json(resp))
}

/// DELETE /x/followed-by/:handle — drop a handle from the pending list.
#[utoipa::path(
    delete, path = XVerificationPath::FollowedByDelete.docs_str(),
    params(("handle" = String, Path, description = "X handle to remove")),
    responses((status = 200, body = PendingResponse)),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address))]
pub async fn delete_followed_by(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(handle): Path<String>,
) -> AppJsonResult<PendingResponse> {
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    let resp = service(&state).remove_followed_by(&account_id, &handle).await?;
    Ok(Json(resp))
}

/// GET /x/verification/pending — current pending signals (no creator handle).
#[utoipa::path(
    get, path = XVerificationPath::Pending.docs_str(),
    responses((status = 200, body = PendingResponse)),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address))]
pub async fn get_pending(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<PendingResponse> {
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    let resp = service(&state).get_pending(&account_id).await?;
    Ok(Json(resp))
}

/// POST /x/verification/finalize — verify ownership (§7.1) + persist.
#[utoipa::path(
    post, path = XVerificationPath::Finalize.docs_str(),
    request_body = FinalizeRequest,
    responses(
        (status = 200, body = FinalizeResponse),
        (status = 403, description = "creator_mismatch"),
        (status = 410, description = "verification_expired")
    ),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address))]
pub async fn finalize(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<FinalizeRequest>,
) -> AppJsonResult<FinalizeResponse> {
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    service(&state).finalize(&payload, &account_id).await?;
    Ok(Json(FinalizeResponse { ok: true }))
}

async fn rate_limit(state: &AppState, id: &str, limit: u64) -> Result<(), AppError> {
    match check_and_increment(&state.redis, id, limit).await? {
        RateLimitResult::Exceeded { retry_after, .. } => {
            Err(AppError::TooManyRequests { retry_after })
        }
        RateLimitResult::Allowed { .. } => Ok(()),
    }
}
```

- [ ] **Step 4: Write `mod.rs`** (Idiom A — build one `auth_layer`, apply to the 5 protected routes; callback stays public)

Create `src/router/x_verification/mod.rs`:

```rust
pub mod handler;
pub mod path;

use axum::{
    Router,
    routing::{delete, get, post},
};
use tower::ServiceBuilder;

use axum::middleware as axum_middleware;

use crate::middleware::authenticate_user;
use crate::state::AppState;
use path::XVerificationPath;

pub fn router(app_state: AppState) -> Router<AppState> {
    let auth_layer = ServiceBuilder::new().layer(axum_middleware::from_fn_with_state(
        app_state,
        authenticate_user,
    ));

    Router::new()
        // Public: X redirects the browser here (no session/Origin).
        .route(XVerificationPath::OauthCallback.as_str(), get(handler::oauth_callback))
        // Session-required routes.
        .route(
            XVerificationPath::OauthLogin.as_str(),
            post(handler::oauth_login).layer(auth_layer.clone()),
        )
        .route(
            XVerificationPath::FollowedBy.as_str(),
            post(handler::add_followed_by).layer(auth_layer.clone()),
        )
        .route(
            XVerificationPath::FollowedByDelete.as_str(),
            delete(handler::delete_followed_by).layer(auth_layer.clone()),
        )
        .route(
            XVerificationPath::Pending.as_str(),
            get(handler::get_pending).layer(auth_layer.clone()),
        )
        .route(
            XVerificationPath::Finalize.as_str(),
            post(handler::finalize).layer(auth_layer),
        )
}
```

- [ ] **Step 5: Declare the router module** — add to `src/router/mod.rs`

```rust
pub mod x_verification;
```

- [ ] **Step 6: Exclude the callback from `api_key_gate`** — in `src/middleware.rs`, add to the early-return path list in `api_key_gate` (next to `|| path.starts_with("/auth/")`)

```rust
        || path == "/x/oauth/callback"    // X redirects here (no Origin / API key)
```

- [ ] **Step 7: Wire into `main.rs`**

Add `x_verification` to the `router::{ … }` import list (lines 4-8).

Add to the merge chain (near the other `app_state.clone()` routers, e.g. after `.merge(account::router(app_state.clone()))`):

```rust
        .merge(x_verification::router(app_state.clone()))
```

Add to the OpenAPI `paths( … )` block:

```rust
        // ----------------X Verification----------------
        router::x_verification::handler::oauth_login,
        router::x_verification::handler::oauth_callback,
        router::x_verification::handler::add_followed_by,
        router::x_verification::handler::delete_followed_by,
        router::x_verification::handler::get_pending,
        router::x_verification::handler::finalize,
```

Add to the OpenAPI `schemas( … )` block (if not already added in Task 4):

```rust
        types::x_verification::OAuthLoginResponse,
        types::x_verification::FollowedByRequest,
        types::x_verification::FollowedByResponse,
        types::x_verification::PendingResponse,
        types::x_verification::FinalizeRequest,
        types::x_verification::FinalizeResponse,
```

- [ ] **Step 8: Build**

Run: `cargo build 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 9: Commit**

```bash
git add src/router/x_verification src/router/mod.rs src/services/rate_limiter.rs src/middleware.rs src/main.rs
git commit -m "feat(x): x_verification router (6 endpoints) + wiring"
```

---

## Task 10: `GET /token/:token` — join + populate `x_verification`

**Files:**
- Modify: `src/controllers/token/mod.rs`
- Test: `src/controllers/token/mod.rs` (`#[sqlx::test]`)

**Interfaces:**
- Consumes: Task 1 tables, Task 4 types.
- Produces: `TokenResponse.token_info.x_verification` populated when a `token_x_verification` row exists.

- [ ] **Step 1: Write the failing tests**

Add to `src/controllers/token/mod.rs` (`#[cfg(test)] mod tests`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const TOKEN: &str = "0x000000000000000000000000000000000000B143";
    const CREATOR: &str = "0x000000000000000000000000000000000000Aa01";

    fn ctrl(pool: PgPool) -> TokenController {
        TokenController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn seed_token(pool: &PgPool) {
        sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,'c','','') ON CONFLICT DO NOTHING")
            .bind(CREATOR).execute(pool).await.unwrap();
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply,version)
            VALUES ($1,'T','T','',$2,NULL,false,false,false,1,'0xh',1000,'V2') ON CONFLICT DO NOTHING"#)
            .bind(TOKEN).bind(CREATOR).execute(pool).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn unverified_token_has_none(pool: PgPool) {
        seed_token(&pool).await;
        let resp = ctrl(pool).get_token(TOKEN).await.unwrap();
        assert!(resp.token_info.x_verification.is_none());
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn verified_token_returns_nested_signals(pool: PgPool) {
        seed_token(&pool).await;
        sqlx::query("INSERT INTO token_x_verification (token_id,account_id,x_user_id,followers_count) VALUES ($1,$2,'9',128000)")
            .bind(TOKEN).bind(CREATOR).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO token_x_followed_by (token_id,x_handle,x_image_uri,x_followers_count) VALUES ($1,'elonmusk','https://img',200000000)")
            .bind(TOKEN).execute(&pool).await.unwrap();

        let resp = ctrl(pool).get_token(TOKEN).await.unwrap();
        let xv = resp.token_info.x_verification.expect("verified");
        assert_eq!(xv.followers_count, 128000);
        assert_eq!(xv.followed_by.len(), 1);
        assert_eq!(xv.followed_by[0].x_handle, "elonmusk");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib controllers::token::tests`
Expected: FAIL — `verified_token_returns_nested_signals` fails (x_verification is None because `fetch_token` doesn't join yet).

- [ ] **Step 3: Add types + a `TokenRow` field** in `src/controllers/token/mod.rs`

Extend the imports:

```rust
use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::{
        common::info::{AccountInfo, TokenInfo, TokenVersion},
        token::{
            TokenResponse,
            x_verification::{TokenXVerification, XFollowedByEntry},
        },
    },
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};
```

Add a field to `struct TokenRow` (after `creator_bio`):

```rust
    creator_bio: String,
    // NULL unless this token has a token_x_verification row.
    x_followers_count: Option<i64>,
```

- [ ] **Step 4: Join in `fetch_token`** — add the SELECT column + LEFT JOIN

In the SQL, after `a.bio as creator_bio,` add:

```sql
                    txv.followers_count as x_followers_count,
```

and after `LEFT JOIN account_x ax ON a.account_id = ax.account_id` add:

```sql
                LEFT JOIN token_x_verification txv ON txv.token_id = t.token_id
```

(Remove the now-unused trailing `a.follower_count`/`a.following_count` SELECT lines only if the compiler warns; they are harmless as extra columns — leave them if unsure.)

- [ ] **Step 5: Fetch followed_by + populate the field** — replace the tail of `fetch_token` (from `let token_info = TokenInfo {` through `Ok(TokenResponse { token_info })`)

```rust
        // Only load followed_by when a verification row exists.
        let x_verification = match row.x_followers_count {
            Some(followers_count) => {
                #[derive(sqlx::FromRow)]
                struct FollowedByRow {
                    x_handle: String,
                    x_image_uri: String,
                    x_followers_count: i64,
                }
                let rows = measure_postgres!(
                    "token.fetch_followed_by",
                    sqlx::query_as::<_, FollowedByRow>(
                        r#"
                        SELECT x_handle, x_image_uri, x_followers_count
                        FROM token_x_followed_by
                        WHERE token_id = $1
                        ORDER BY checked_at ASC
                        "#,
                    )
                    .bind(token_id)
                    .fetch_all(self.db.get_read_pool())
                )
                .map_err(|err| anyhow!("Failed to get followed_by: {}", err))?;

                Some(TokenXVerification {
                    followers_count,
                    followed_by: rows
                        .into_iter()
                        .map(|r| XFollowedByEntry {
                            x_handle: r.x_handle,
                            x_image_uri: r.x_image_uri,
                            x_followers_count: r.x_followers_count,
                        })
                        .collect(),
                })
            }
            None => None,
        };

        let token_info = TokenInfo {
            token_id: row.token_id,
            name: row.name,
            symbol: row.symbol,
            image_uri: row.image_uri,
            description: row.description,
            is_graduated: row.is_graduated,
            is_nsfw: row.is_nsfw,
            twitter: row.twitter.filter(|s| !s.is_empty()),
            telegram: row.telegram.filter(|s| !s.is_empty()),
            website: row.website.filter(|s| !s.is_empty()),
            created_at: row.created_at,
            creator: AccountInfo {
                account_id: row.creator,
                nickname: row.creator_nickname,
                bio: row.creator_bio,
                image_uri: row.creator_image_uri,
            },
            is_cto: row.is_cto,
            version: row.version.clone(),
            x_verification,
        };

        Ok(TokenResponse { token_info })
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --lib controllers::token::tests`
Expected: PASS — both `unverified_token_has_none` and `verified_token_returns_nested_signals`.

- [ ] **Step 7: Note the cache caveat** (no code change) — `TokenService::get_token` caches `TokenResponse` in Redis for `GET_TOKEN_RESPONSE_EXPIRATION` (60s) and `TokenController::get_token` wraps `fetch_token` in an in-process single-flight cache. A newly finalized token's `x_verification` becomes visible after the ≤60s Redis TTL expires (whole `TokenResponse` is serialized, so no field-level change is needed). Acceptable for MVP; do not add cache invalidation.

- [ ] **Step 8: Commit**

```bash
git add src/controllers/token/mod.rs
git commit -m "feat(x): expose x_verification on GET /token/:token"
```

---

## Task 11: Full-suite verification

**Files:** none (verification only).

- [ ] **Step 1: Format + lint**

Run: `cargo fmt --all && cargo clippy --all-targets 2>&1 | tail -30`
Expected: no errors (warnings acceptable; fix any `clippy::` errors introduced by this feature).

- [ ] **Step 2: Full test run**

Run: `cargo test 2>&1 | tail -40`
Expected: all tests pass, including `finalize_checks`, `x_oauth::client::tests`, `x_verification` type tests, controller `#[sqlx::test]`s, and token-detail JOIN tests. (DB-backed `#[sqlx::test]`s require a reachable local Postgres, per the existing suite.)

- [ ] **Step 3: Manual smoke of the public shape (optional)** — with the server running and env set, confirm `GET /token/{a-finalized-token}` returns `token_info.x_verification` and that no response anywhere contains the creator's own handle or `x_user_id`.

- [ ] **Step 4: Commit any fmt/clippy fixups**

```bash
git add -A
git commit -m "chore(x): fmt + clippy"
```

---

## Task 12: C1 remediation — `reserve` endpoint + `finalize` simplified to reservation-only

> **Context (read first):** Tasks 1–11 shipped and passed review, but the whole-branch review found a CRITICAL flaw (**C1**). `compute_create2_address(deployer, implementation, salt)` does **not** use `creator` (deployer/implementation are env-loaded per-version constants; `creator` only seeds salt mining — see memory `reference_token_salt_create2`). So `finalize`'s two checks (`creator == session`, `CREATE2(salt) == token_id`) only prove *"the caller knows this salt"*, not *"the caller is the true creator"*: `creator` is attacker-suppliable (they set it to their own session), and **salt becomes public on-chain once the token deploys** (`BondingCurveRouter.create` calldata). Post-deploy, anyone can read the salt and finalize arbitrary coins (`ON CONFLICT (token_id) DO UPDATE` even overwrites). Design doc §7.1-B has the full write-up.
>
> **Fix (approved):** add `POST /x/verification/reserve`, called right after `/token/salt` (pre-deploy, salt still secret). It re-runs creator/CREATE2 checks *and* verifies the token is **not yet deployed** (`eth_getCode` empty, fail-closed) *and* records a first-writer-wins `token_id → account_id` reservation. `finalize` is then **simplified to `{token_id}`-only**: it just checks the reservation belongs to the session. This also eliminates **I1** (`FinalizeRequest.version` defaulting to V1 → V2 tokens always 403): `version` moves to `ReserveRequest` (required, no serde default) and leaves `finalize` entirely.
>
> **Design decision — finalize is reservation-only (CREATE2 NOT re-checked in finalize):** re-running CREATE2 in finalize would only re-prove salt-knowledge, the exact signal C1 shows is worthless post-deploy; the real authority is the pre-deploy reservation. The CREATE2/creator logic is **relocated to reserve, not deleted** — `SaltService::compute_token_address`, `creator_matches`, `address_matches`, `canonical_persist_ids` are all reused by reserve, so their existing (reviewed) tests stay valid. Rationale in design §7.1-B.

**Files (whole task):**
- Modify: `src/result.rs` — `Conflict(String)` (convert from unit variant) + new `ServiceUnavailable(String)`.
- Create: `migrations/0037_token_x_reservation.sql` + `migrations/v2_upgrade_token_x_reservation.sql` (submodule branch `feat/token-x-verification`).
- Create: `migrations-test/0037_token_x_reservation.sql` (symlink).
- Modify: `src/services/rate_limiter.rs` — `X_RESERVE_RATE_LIMIT`.
- Modify: `.env.example` — document the already-required `RPC_URL` (now also used at runtime by reserve).
- Create: `src/services/x_verification/onchain.rs` — on-chain deploy check (pure `code_indicates_deployed` + RPC `is_contract_deployed`).
- Modify: `src/services/x_verification/mod.rs` — declare `onchain`, add `reserve()`, rewrite `finalize()` to reservation-only.
- Modify: `src/types/x_verification/mod.rs` — add `ReserveRequest`/`ReserveResponse`, shrink `FinalizeRequest` to `{ token_id }`.
- Modify: `src/controllers/x_verification/mod.rs` — `reserve_first_writer` + `reservation_owner` (+ `#[sqlx::test]`s).
- Modify: `src/router/x_verification/{path.rs,handler.rs,mod.rs}` — add the reserve route; adjust finalize handler for the shrunk request.
- Modify: `src/main.rs` — OpenAPI `paths` + `schemas` for reserve.

**Interfaces (whole task):**
- Produces: `AppError::{Conflict(String), ServiceUnavailable(String)}`; `services::x_verification::onchain::{code_indicates_deployed, is_contract_deployed}`; `XVerificationController::{reserve_first_writer, reservation_owner}`; `XVerificationService::reserve`; `types::x_verification::{ReserveRequest, ReserveResponse}`; route `POST /x/verification/reserve`.
- Consumes (relocated from finalize): `SaltService::compute_token_address`, `creator_matches`, `address_matches`, `canonical_persist_ids` (all already defined in Tasks 6/7); `config::RPC_URL`; `services::rate_limiter::check_and_increment`.

---

### Task 12.1: Error variants — `Conflict(String)` (409) + `ServiceUnavailable(String)` (503)

**Files:**
- Modify: `src/result.rs`
- Test: `src/result.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `AppError::Conflict(String)` → HTTP 409 `{"error": msg}`; `AppError::ServiceUnavailable(String)` → HTTP 503 `{"error": msg}`. Consumed by Tasks 12.4 / 12.7.
- Note: the existing `AppError::Conflict` is a **unit** variant with **no construction sites** (verified: `grep -rn "Conflict" src/` shows only the declaration + its match arm). Converting it to `Conflict(String)` is safe.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)]` area of `src/result.rs` (reuse/extend the `error_status_tests` module added in Task 3):

```rust
    #[test]
    fn conflict_and_service_unavailable_map_to_409_and_503() {
        assert_eq!(
            AppError::Conflict("already_deployed".into()).into_response().status(),
            StatusCode::CONFLICT
        );
        assert_eq!(
            AppError::ServiceUnavailable("onchain_check_unavailable".into())
                .into_response()
                .status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test conflict_and_service_unavailable_map_to_409_and_503`
Expected: FAIL — `this enum variant takes 0 arguments` (for `Conflict`) / `no variant named ServiceUnavailable`.

- [ ] **Step 3: Convert `Conflict` + add `ServiceUnavailable`** in `enum AppError` (`src/result.rs`)

Change the unit variant to a bodied one and add the new variant next to it:

```rust
    Conflict(String),
    ServiceUnavailable(String),
```

- [ ] **Step 4: Update the match arms** in `impl IntoResponse for AppError`

Replace the existing bodiless `Conflict` arm and add the 503 arm:

```rust
            AppError::Conflict(msg) => (StatusCode::CONFLICT, msg),
            AppError::ServiceUnavailable(msg) => (StatusCode::SERVICE_UNAVAILABLE, msg),
```

(These join the plain `(status, message)` arms that fall through to the shared `Json(json!({ "error": error_message }))` body — same shape as `BadRequest`/`Forbidden`.)

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test conflict_and_service_unavailable_map_to_409_and_503`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/result.rs
git commit -m "feat(error): Conflict(String) 409 + ServiceUnavailable(String) 503"
```

---

### Task 12.2: Migrations submodule — `token_x_reservation` table + test track + gitlink bump

**Files:**
- Create: `migrations/0037_token_x_reservation.sql` (submodule)
- Create: `migrations/v2_upgrade_token_x_reservation.sql` (submodule)
- Create: `migrations-test/0037_token_x_reservation.sql` (symlink)

**Interfaces:**
- Produces: table `token_x_reservation(token_id PK, account_id, reserved_at)`. Consumed by Task 12.6 (controller) and 12.7 (service).

- [ ] **Step 1: Confirm the submodule branch + next free number**

Run: `git -C /Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification/migrations status -sb && git -C /Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification/migrations log --oneline -1 && ls /Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification/migrations | grep -E '^00' | sort | tail -2`
Expected: on branch `feat/token-x-verification`, HEAD is `714a191 feat(x): token_x_verification + token_x_followed_by tables`, highest number is `0036_token_x_verification.sql` (so `0037_` is free). This is the SAME open feature branch that carries Task 1's `0036` — commit the new migration onto it (it is not merged; there is no reason to branch again). If HEAD/branch differ, adapt: create/checkout `feat/token-x-verification` and use the next free `00NN`.

- [ ] **Step 2: Write the install migration**

Create `/Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification/migrations/0037_token_x_reservation.sql`:

```sql
-- 0037_token_x_reservation.sql
--
-- Pre-deploy reservation binding a CREATE2 token_id to a session account while
-- the salt is still secret (before on-chain deployment). finalize checks only
-- that this reservation belongs to the session (design §3-bis, §7.1-B).
--
-- No FK to `token` or `token_x_verification`: the reservation is created BEFORE
-- either row exists. token_id is stored EIP-55 checksummed (never LOWER()).
-- The row is immutable once created (account_id never changes) — this is what
-- makes the reserve INSERT a sound first-writer-wins record.
CREATE TABLE IF NOT EXISTS token_x_reservation (
    token_id VARCHAR(42) PRIMARY KEY,
    account_id VARCHAR(42) NOT NULL,
    reserved_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_token_x_reservation_account_id
    ON token_x_reservation (account_id);
```

- [ ] **Step 3: Write the prod upgrade-track migration** (identical, idempotent)

Create `/Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification/migrations/v2_upgrade_token_x_reservation.sql` with the **same** `CREATE TABLE IF NOT EXISTS` + `CREATE INDEX IF NOT EXISTS` statements as Step 2 (copy them verbatim; `IF NOT EXISTS` makes it safe against a live DB). This mirrors the `0036` + `v2_upgrade_token_x_verification.sql` pair from Task 1.

- [ ] **Step 4: Commit + push the submodule branch**

```bash
git -C /Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification/migrations add 0037_token_x_reservation.sql v2_upgrade_token_x_reservation.sql
git -C /Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification/migrations commit -m "feat(x): token_x_reservation table (pre-deploy first-writer reservation)"
git -C /Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification/migrations push origin feat/token-x-verification
```

- [ ] **Step 5: Wire the test track (symlink)**

```bash
ln -s ../migrations/0037_token_x_reservation.sql \
  /Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification/migrations-test/0037_token_x_reservation.sql
```

Run: `ls -la /Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification/migrations-test/0037_token_x_reservation.sql`
Expected: a symlink `-> ../migrations/0037_token_x_reservation.sql`.

- [ ] **Step 6: Verify the schema applies from scratch**

Add a throwaway smoke test (delete after Task 12.6 lands its real tests) to `src/controllers/x_verification/mod.rs`:

```rust
#[cfg(test)]
mod reservation_migration_smoke {
    use sqlx::PgPool;
    #[sqlx::test(migrations = "./migrations-test")]
    async fn reservation_table_exists(pool: PgPool) {
        let (ok,): (bool,) =
            sqlx::query_as("SELECT to_regclass('token_x_reservation') IS NOT NULL")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(ok, "token_x_reservation must exist");
    }
}
```

Run: `cargo test reservation_table_exists -- --nocapture`
Expected: PASS (requires a local Postgres reachable by `#[sqlx::test]`).

- [ ] **Step 7: Bump the parent gitlink**

```bash
git -C /Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification add migrations migrations-test
git -C /Users/gyu/project/nads-pump/api-server/.worktrees/x-hidden-creator-verification commit -m "chore(migrations): bump submodule for token_x_reservation (#gitlink)"
```

---

### Task 12.3: Rate-limit const + `.env.example` `RPC_URL`

**Files:**
- Modify: `src/services/rate_limiter.rs`
- Modify: `.env.example`

**Interfaces:**
- Produces: `services::rate_limiter::X_RESERVE_RATE_LIMIT: u64`. Consumed by Task 12.8.

- [ ] **Step 1: Add the const** to `src/services/rate_limiter.rs` (next to `X_FOLLOWED_BY_RATE_LIMIT`)

```rust
/// X reservation attempts per account per minute (each does an on-chain RPC call).
pub const X_RESERVE_RATE_LIMIT: u64 = 5;
```

- [ ] **Step 2: Document `RPC_URL`** — append to `.env.example`

`RPC_URL` is already `.expect()`-required by `src/config.rs` but was missing from `.env.example`; reserve's on-chain deploy check makes it a runtime dependency of this feature, so document it now:

```bash
# EVM JSON-RPC endpoint (required). Used for balance/code reads, incl. the
# X-verification reserve pre-deploy check (eth_getCode).
RPC_URL=https://testnet-rpc.monad.xyz
```

- [ ] **Step 3: Build check**

Run: `cargo build 2>&1 | tail -5`
Expected: clean (const is trivially valid).

- [ ] **Step 4: Commit**

```bash
git add src/services/rate_limiter.rs .env.example
git commit -m "feat(x): X_RESERVE_RATE_LIMIT + document RPC_URL in .env.example"
```

---

### Task 12.4: On-chain deploy check (pure logic + RPC boundary, fail-closed)

**Files:**
- Create: `src/services/x_verification/onchain.rs`
- Modify: `src/services/x_verification/mod.rs` (add `pub mod onchain;` at the top)
- Test: `src/services/x_verification/onchain.rs` (`#[cfg(test)]` — pure part only)

**Interfaces:**
- Produces:
  - `pub fn code_indicates_deployed(code: &[u8]) -> bool` (pure; deployed ⇔ non-empty bytecode).
  - `pub async fn is_contract_deployed(token_id: &str) -> Result<bool, AppError>` (RPC; **fail-closed** — any RPC error ⇒ `Err(ServiceUnavailable)`).
- Consumes: `config::RPC_URL`; `result::AppError` (needs `ServiceUnavailable` from Task 12.1).
- Testing note: the RPC call is not unit-tested (no existing mock harness — `src/services/pricing/balance.rs` likewise leaves its `RpcBalanceSource::fetch` to integration/manual). The **pure decision** (`code_indicates_deployed`) is unit-tested; the RPC wrapper is a thin, mockless boundary verified by Task 12.9's manual smoke.

- [ ] **Step 1: Write the pure-logic failing test + the module**

Create `src/services/x_verification/onchain.rs`:

```rust
//! On-chain deploy check for the pre-deploy reservation (design §7.1-B).
//!
//! The security boundary: a reservation may only be created while the token is
//! NOT yet deployed (its salt is therefore still secret). We read `eth_getCode`
//! and treat ANY RPC failure as "cannot confirm undeployed" ⇒ reject the
//! reservation (fail-closed). Never optimistically assume "not deployed".

use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};

use crate::config::RPC_URL;
use crate::result::AppError;

/// Pure: an address already holds contract code ⇒ it is deployed.
pub fn code_indicates_deployed(code: &[u8]) -> bool {
    !code.is_empty()
}

/// Fetch `eth_getCode(token_id)` (latest block) and report whether the address
/// is already deployed. Fail-closed: any RPC/parse error ⇒ `ServiceUnavailable`
/// so the caller rejects the reservation (a chain-node outage must NOT be read
/// as "undeployed" — that would reopen C1).
pub async fn is_contract_deployed(token_id: &str) -> Result<bool, AppError> {
    let rpc_url: url::Url = RPC_URL
        .parse()
        .map_err(|e| AppError::InternalError(format!("Invalid RPC_URL: {e}")))?;
    let provider = ProviderBuilder::new().connect_http(rpc_url);
    let addr: Address = token_id
        .parse()
        .map_err(|_| AppError::BadRequest("invalid token_id".into()))?;
    let code = provider.get_code_at(addr).await.map_err(|e| {
        AppError::ServiceUnavailable(format!("onchain_check_unavailable: {e}"))
    })?;
    Ok(code_indicates_deployed(&code))
}

#[cfg(test)]
mod tests {
    use super::code_indicates_deployed;

    #[test]
    fn empty_code_is_not_deployed() {
        assert!(!code_indicates_deployed(&[]));
    }

    #[test]
    fn nonempty_code_is_deployed() {
        // A tiny slice of real runtime bytecode prefix.
        assert!(code_indicates_deployed(&[0x60, 0x80, 0x60, 0x40, 0x52]));
    }
}
```

- [ ] **Step 2: Declare the module** — add to the TOP of `src/services/x_verification/mod.rs` (before the `use` block)

```rust
pub mod onchain;
```

- [ ] **Step 3: Run the pure tests**

Run: `cargo test --lib x_verification::onchain`
Expected: PASS — `empty_code_is_not_deployed`, `nonempty_code_is_deployed`. (`get_code_at` is on the `alloy::providers::Provider` trait, imported above; it returns `alloy::primitives::Bytes`, which derefs to `&[u8]`.)

- [ ] **Step 4: Commit**

```bash
git add src/services/x_verification/onchain.rs src/services/x_verification/mod.rs
git commit -m "feat(x): on-chain deploy check (fail-closed eth_getCode)"
```

---

### Task 12.5: Reservation DTOs + shrink `FinalizeRequest`

**Files:**
- Modify: `src/types/x_verification/mod.rs`
- Test: `src/types/x_verification/mod.rs` (`#[cfg(test)]`)

**Interfaces:**
- Produces: `ReserveRequest { token_id, creator, salt, version }` (**`version` required — no serde default**, resolving I1), `ReserveResponse { ok: bool }`. `FinalizeRequest` shrinks to `{ token_id: String }`.
- Consumes: `types::common::info::TokenVersion`.

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` in `src/types/x_verification/mod.rs`:

```rust
    #[test]
    fn reserve_request_requires_version() {
        // Missing `version` must FAIL to deserialize (no silent V1 default → I1 fix).
        let missing = r#"{"token_id":"0xabc","creator":"0xdef","salt":"0x01"}"#;
        assert!(serde_json::from_str::<ReserveRequest>(missing).is_err());

        let ok = r#"{"token_id":"0xabc","creator":"0xdef","salt":"0x01","version":"V2"}"#;
        let req: ReserveRequest = serde_json::from_str(ok).unwrap();
        assert!(matches!(req.version, TokenVersion::V2));
    }

    #[test]
    fn finalize_request_is_token_id_only() {
        let req: FinalizeRequest = serde_json::from_str(r#"{"token_id":"0xabc"}"#).unwrap();
        assert_eq!(req.token_id, "0xabc");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib x_verification::tests::reserve_request_requires_version x_verification::tests::finalize_request_is_token_id_only`
Expected: FAIL — `cannot find type ReserveRequest` and/or `FinalizeRequest` still has extra required fields.

- [ ] **Step 3: Add the reserve DTOs** to `src/types/x_verification/mod.rs` (near `FinalizeRequest`)

```rust
/// Pre-deploy reservation request (design §3-bis). Sent right after
/// `/token/salt`, before on-chain deployment. `version` is REQUIRED (no serde
/// default) so a V2 token is never silently CREATE2-checked against V1
/// impl/deployer (resolves I1). `salt` proves salt-possession; `creator` must
/// equal the session wallet.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ReserveRequest {
    pub token_id: String,
    pub creator: String,
    pub salt: String,
    pub version: TokenVersion,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ReserveResponse {
    pub ok: bool,
}
```

- [ ] **Step 4: Shrink `FinalizeRequest`** in the same file

Replace the existing `FinalizeRequest` struct (and delete the now-unused `default_version` helper **only if nothing else in this file references it** — `ReserveRequest.version` has no default, so `default_version` is likely dead; remove it if `grep -n default_version src/types/x_verification/mod.rs` shows only its own definition):

```rust
/// Finalize is reservation-only (design §7.1-B): the CREATE2/creator/salt/
/// version checks moved to `reserve`. Only the token_id is needed; ownership is
/// proven by the pre-existing reservation.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct FinalizeRequest {
    pub token_id: String,
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib x_verification::tests`
Expected: PASS (new tests + the existing `handle_validation`/`append_enforces_max_and_dedup`). If the compiler flags an unused import (e.g. the removed `default_version` referenced `TokenVersion`), note `TokenVersion` is still used by `ReserveRequest` — keep the import.

- [ ] **Step 6: Commit**

```bash
git add src/types/x_verification/mod.rs
git commit -m "feat(x): ReserveRequest/ReserveResponse + shrink FinalizeRequest to token_id"
```

---

### Task 12.6: Reservation controller — first-writer-wins INSERT + owner lookup

**Files:**
- Modify: `src/controllers/x_verification/mod.rs`
- Test: `src/controllers/x_verification/mod.rs` (`#[sqlx::test]`)

**Interfaces:**
- Produces:
  - `async fn reserve_first_writer(&self, token_id: &str, account_id: &str) -> anyhow::Result<String>` — atomic upsert returning the **surviving owner** account_id (the freshly-inserted one, or the pre-existing one on conflict). Single statement ⇒ race-free.
  - `async fn reservation_owner(&self, token_id: &str) -> anyhow::Result<Option<String>>` — read the owner (uses the **write pool** to avoid replica lag on this security gate; see memory `reference_cms_write_admin_writepool`).
- Consumes: Task 12.2 table.

- [ ] **Step 1: Write the failing tests**

Add a second `#[sqlx::test]` cluster to `src/controllers/x_verification/mod.rs` (the `ctrl`, `TOKEN`, `ACCOUNT`, `ACCOUNT2` fixtures from Task 8 already exist in this file's `mod tests`; add these there):

```rust
    #[sqlx::test(migrations = "./migrations-test")]
    async fn reserve_is_first_writer_wins_and_idempotent(pool: PgPool) {
        let c = ctrl(pool.clone());
        // first writer wins
        let owner = c.reserve_first_writer(TOKEN, ACCOUNT).await.unwrap();
        assert_eq!(owner, ACCOUNT, "first reservation records the caller");
        // same account retry is idempotent (still owner)
        let owner2 = c.reserve_first_writer(TOKEN, ACCOUNT).await.unwrap();
        assert_eq!(owner2, ACCOUNT);
        // a different account cannot take it — returns the ORIGINAL owner
        let owner3 = c.reserve_first_writer(TOKEN, ACCOUNT2).await.unwrap();
        assert_eq!(owner3, ACCOUNT, "conflict returns the first writer, not the challenger");
        // exactly one row, still owned by the first writer
        let (n,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM token_x_reservation WHERE token_id = $1")
                .bind(TOKEN)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(n, 1);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn reservation_owner_reads_back(pool: PgPool) {
        let c = ctrl(pool.clone());
        assert!(c.reservation_owner(TOKEN).await.unwrap().is_none());
        c.reserve_first_writer(TOKEN, ACCOUNT).await.unwrap();
        assert_eq!(c.reservation_owner(TOKEN).await.unwrap().as_deref(), Some(ACCOUNT));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib x_verification::tests::reserve_is_first_writer_wins_and_idempotent x_verification::tests::reservation_owner_reads_back`
Expected: FAIL — `no method named reserve_first_writer`.

- [ ] **Step 3: Add the methods** to `impl XVerificationController` in `src/controllers/x_verification/mod.rs` (after `finalize`)

```rust
    /// Atomic first-writer-wins reservation. Returns the SURVIVING owner:
    /// the freshly-inserted account on success, or the pre-existing owner on
    /// conflict. The `DO UPDATE SET account_id = <its current value>` no-op
    /// forces `RETURNING` to yield the surviving row in one statement (no
    /// read-then-write race). The row is otherwise immutable.
    pub async fn reserve_first_writer(
        &self,
        token_id: &str,
        account_id: &str,
    ) -> Result<String> {
        let (owner,): (String,) = sqlx::query_as(
            r#"
            INSERT INTO token_x_reservation (token_id, account_id)
            VALUES ($1, $2)
            ON CONFLICT (token_id) DO UPDATE
                SET account_id = token_x_reservation.account_id
            RETURNING account_id
            "#,
        )
        .bind(token_id)
        .bind(account_id)
        .fetch_one(self.db.get_write_pool())
        .await
        .map_err(|e| anyhow!("reserve first-writer: {e}"))?;
        Ok(owner)
    }

    /// Read the reservation owner. Uses the WRITE pool so the finalize gate is
    /// read-after-write consistent (reserve → finalize can be near-simultaneous;
    /// a read replica could lag and wrongly report "not reserved").
    pub async fn reservation_owner(&self, token_id: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT account_id FROM token_x_reservation WHERE token_id = $1")
                .bind(token_id)
                .fetch_optional(self.db.get_write_pool())
                .await
                .map_err(|e| anyhow!("reservation owner: {e}"))?;
        Ok(row.map(|(a,)| a))
    }
```

- [ ] **Step 4: Remove the throwaway smoke test** from Task 12.2 Step 6 (`reservation_migration_smoke`) — these real tests supersede it.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib x_verification::tests::reserve_is_first_writer_wins_and_idempotent x_verification::tests::reservation_owner_reads_back`
Expected: PASS (requires local Postgres).

- [ ] **Step 6: Commit**

```bash
git add src/controllers/x_verification/mod.rs
git commit -m "feat(x): reservation controller (first-writer-wins + owner lookup)"
```

---

### Task 12.7: Service — `reserve()` orchestration + `finalize()` reservation-only

**Files:**
- Modify: `src/services/x_verification/mod.rs`
- Test: `src/services/x_verification/mod.rs` (`#[cfg(test)]` — the relocated pure checks stay; see note)

**Interfaces:**
- Produces: `XVerificationService::reserve(&self, req: &ReserveRequest, session_address: &str) -> Result<(), AppError>`.
- Modifies: `XVerificationService::finalize` signature stays `(&self, req: &FinalizeRequest, session_address: &str)` but body becomes reservation-only.
- Consumes: `onchain::is_contract_deployed` (12.4); `XVerificationController::{reserve_first_writer, reservation_owner}` (12.6); relocated `creator_matches`/`address_matches`/`canonical_persist_ids`/`SaltService::compute_token_address` (already defined).

- [ ] **Step 1: Add the reserve imports** to `src/services/x_verification/mod.rs`

Extend the `use crate::types::x_verification::{…}` line to include `ReserveRequest`, and add the onchain import:

```rust
use crate::services::x_verification::onchain;
use crate::types::x_verification::{
    FinalizeRequest, FollowedByResponse, OAuthLoginResponse, PendingResponse, ReserveRequest,
    XOAuthState, XPending, append_followed_by,
};
```

- [ ] **Step 2: Add `reserve()` to `impl XVerificationService`** (place it just before `finalize`)

```rust
    /// Pre-deploy reservation (design §3-bis, §7.1-B). Runs the (relocated)
    /// creator + CREATE2 checks, confirms the token is NOT yet deployed on-chain
    /// (fail-closed), then records an atomic first-writer-wins reservation.
    pub async fn reserve(
        &self,
        req: &ReserveRequest,
        session_address: &str,
    ) -> Result<(), AppError> {
        // 1. creator == session wallet (cheap first filter).
        creator_matches(&req.creator, session_address)?;
        // 2. CREATE2(version, salt) == token_id (salt-possession proof).
        let salt =
            B256::from_str(&req.salt).map_err(|_| AppError::BadRequest("invalid salt".into()))?;
        let expected = SaltService::compute_token_address(req.version.clone(), salt)?;
        address_matches(expected, &req.token_id)?;
        // 3. canonical checksummed ids (never LOWER()).
        let (token_id_cs, account_id_cs) = canonical_persist_ids(expected, session_address)?;
        // 4. THE boundary: token must NOT already be deployed (salt still secret).
        //    Fail-closed — an RPC error propagates as 503, never "assumed undeployed".
        if onchain::is_contract_deployed(&token_id_cs).await? {
            return Err(AppError::Conflict("already_deployed".into()));
        }
        // 5. first-writer-wins reservation.
        let controller = XVerificationController::new(self.postgres.clone());
        let owner = controller
            .reserve_first_writer(&token_id_cs, &account_id_cs)
            .await
            .map_err(|e| {
                error!("reserve persist failed: {e}");
                AppError::InternalError("reserve failed".into())
            })?;
        if owner != account_id_cs {
            return Err(AppError::Conflict("token_already_reserved".into()));
        }
        Ok(())
    }
```

- [ ] **Step 3: Rewrite `finalize()`** — replace the whole method body with the reservation-only version

```rust
    /// Reservation-only finalize (design §7.1-B). The CREATE2/creator checks now
    /// live in `reserve`; here we only prove the token_id was reserved by THIS
    /// session, then persist the pending signals. The reservation row is kept
    /// (not deleted) so re-finalize stays idempotent and only the reserver may
    /// ever refresh this token's verification.
    pub async fn finalize(
        &self,
        req: &FinalizeRequest,
        session_address: &str,
    ) -> Result<(), AppError> {
        // Canonicalize the client token_id + session to checksummed forms.
        let expected = Address::from_str(&req.token_id)
            .map_err(|_| AppError::BadRequest("invalid token_id".into()))?;
        let (token_id_cs, account_id_cs) = canonical_persist_ids(expected, session_address)?;

        // 1. reservation must exist AND belong to this session.
        let controller = XVerificationController::new(self.postgres.clone());
        let owner = controller
            .reservation_owner(&token_id_cs)
            .await
            .map_err(|e| {
                error!("reservation lookup failed: {e}");
                AppError::InternalError("finalize failed".into())
            })?
            .ok_or_else(|| AppError::Forbidden("not_reserved".into()))?;
        if owner != account_id_cs {
            return Err(AppError::Forbidden("not_reserved".into()));
        }

        // 2. pending must still exist.
        let pending = self
            .redis
            .get_x_pending(session_address)
            .await
            .map_err(|e| AppError::InternalError(format!("read pending: {e}")))?
            .ok_or_else(|| AppError::Gone("verification_expired".into()))?;

        // 3. persist (idempotent upsert) — canonical checksummed ids only.
        controller
            .finalize(
                &token_id_cs,
                &account_id_cs,
                &pending.x_user_id,
                pending.followers_count,
                &pending.followed_by,
            )
            .await
            .map_err(|e| {
                error!("finalize persist failed: {e}");
                AppError::InternalError("finalize failed".into())
            })?;

        // 4. clear pending (reservation row intentionally kept).
        let _ = self.redis.delete_x_pending(session_address).await;
        Ok(())
    }
```

- [ ] **Step 4: Confirm the pure-check tests still hold**

No test change needed: the `finalize_checks` module (Task 7) tests `creator_matches`, `address_matches`, `canonical_persist_ids` — all still defined and now exercised via `reserve`. Add one guard comment at the top of that test module:

```rust
    // NOTE (Task 12): these pure helpers are now called by `reserve` (not
    // `finalize`); finalize is reservation-only. The checks are unchanged.
```

- [ ] **Step 5: Run the service tests + build**

Run: `cargo test --lib x_verification::finalize_checks && cargo build 2>&1 | tail -20`
Expected: `finalize_checks` PASS; build clean. If `B256`/`FromStr`/`SaltService` now appear "unused" they do not — `reserve` uses `B256`/`FromStr`/`SaltService`; `finalize` uses `Address`/`FromStr`. Keep all existing imports.

- [ ] **Step 6: Commit**

```bash
git add src/services/x_verification/mod.rs
git commit -m "feat(x): reserve() orchestration + finalize() reservation-only (C1 fix)"
```

---

### Task 12.8: Router — `POST /x/verification/reserve` + finalize handler + OpenAPI

**Files:**
- Modify: `src/router/x_verification/path.rs`
- Modify: `src/router/x_verification/handler.rs`
- Modify: `src/router/x_verification/mod.rs`
- Modify: `src/main.rs`

**Interfaces:**
- Produces route: `POST /x/verification/reserve` (session-required, rate-limited). Consumes Task 12.5 (`ReserveRequest`/`ReserveResponse`), 12.7 (`reserve`), 12.3 (`X_RESERVE_RATE_LIMIT`).

- [ ] **Step 1: Add the path variant** to `src/router/x_verification/path.rs`

Add `Reserve` to the enum and to BOTH `as_str` and `docs_str` (identical string — no path params):

```rust
    OauthLogin,
    OauthCallback,
    FollowedBy,
    FollowedByDelete,
    Pending,
    Reserve,
    Finalize,
```

```rust
            XVerificationPath::Reserve => "/x/verification/reserve",
```

(add the same arm to both `as_str` and `docs_str`).

- [ ] **Step 2: Add the reserve handler** to `src/router/x_verification/handler.rs`

Extend the `types::x_verification::{…}` import with `ReserveRequest, ReserveResponse`, extend the `rate_limiter::{…}` import with `X_RESERVE_RATE_LIMIT`, then add:

```rust
/// POST /x/verification/reserve — pre-deploy first-writer reservation (C1 fix).
/// `skip(payload)` on the span so the salt is never logged.
#[utoipa::path(
    post, path = XVerificationPath::Reserve.docs_str(),
    request_body = ReserveRequest,
    responses(
        (status = 200, body = ReserveResponse),
        (status = 403, description = "creator_mismatch"),
        (status = 409, description = "already_deployed | token_already_reserved"),
        (status = 503, description = "onchain_check_unavailable")
    ),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn reserve(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<ReserveRequest>,
) -> AppJsonResult<ReserveResponse> {
    rate_limit(&state, &format!("x_reserve:{session_address}"), X_RESERVE_RATE_LIMIT).await?;
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    service(&state).reserve(&payload, &account_id).await?;
    Ok(Json(ReserveResponse { ok: true }))
}
```

(The existing `finalize` handler needs **no change** — it already does `Json(payload): Json<FinalizeRequest>` then `service.finalize(&payload, &account_id)`; the shrunk `FinalizeRequest` still deserializes and the call is identical.)

- [ ] **Step 3: Mount the route** in `src/router/x_verification/mod.rs` (session-required, like the other protected routes)

```rust
        .route(
            XVerificationPath::Reserve.as_str(),
            post(handler::reserve).layer(auth_layer.clone()),
        )
```

(Place it next to the `Finalize` route. Ensure the `Finalize` route's `auth_layer` stays the last consumer or clone as needed — use `auth_layer.clone()` on reserve and keep the final route consuming the last `auth_layer`.)

- [ ] **Step 4: Register OpenAPI** in `src/main.rs`

Add to the `paths( … )` X Verification block:

```rust
        router::x_verification::handler::reserve,
```

Add to the `schemas( … )` block:

```rust
        types::x_verification::ReserveRequest,
        types::x_verification::ReserveResponse,
```

- [ ] **Step 5: Build**

Run: `cargo build 2>&1 | tail -20`
Expected: clean build.

- [ ] **Step 6: Commit**

```bash
git add src/router/x_verification src/main.rs
git commit -m "feat(x): POST /x/verification/reserve route + OpenAPI"
```

---

### Task 12.9: Full-suite verification + C1-closure audit

**Files:** none (verification only).

- [ ] **Step 1: Format + lint**

Run: `cargo fmt --all && cargo clippy --all-targets 2>&1 | tail -30`
Expected: no clippy **errors** introduced by this task (warnings acceptable; fix any new `clippy::` errors).

- [ ] **Step 2: Full test run**

Run: `cargo test 2>&1 | tail -40`
Expected: all pass, including `conflict_and_service_unavailable_map_to_409_and_503`, `x_verification::onchain::tests`, `reserve_request_requires_version`, `finalize_request_is_token_id_only`, `reserve_is_first_writer_wins_and_idempotent`, `reservation_owner_reads_back`, and the unchanged `finalize_checks`. (DB-backed `#[sqlx::test]`s need a reachable local Postgres.)

- [ ] **Step 3: Audit — finalize no longer performs CREATE2/creator, and the defense lives only in reserve**

Run: `grep -n "compute_token_address\|creator_matches\|address_matches\|is_contract_deployed" src/services/x_verification/mod.rs`
Expected: all four appear **inside `reserve`** (and the pure helpers' definitions/tests); **none inside `finalize`**. Confirm `finalize` references only `reservation_owner`, `canonical_persist_ids`, `get_x_pending`, and the persistence `finalize`.

Run: `grep -n "version\|salt\|creator" src/types/x_verification/mod.rs | grep -i finalize` and manually confirm `FinalizeRequest` has only `token_id`.

- [ ] **Step 4: Manual smoke (optional, needs server + env + RPC)**

Confirm the happy path: `/token/salt` → `POST /x/verification/reserve` (200) → deploy would now be blocked from re-reservation; a second `reserve` for the same undeployed token from a different session returns 409 `token_already_reserved`; `reserve` for an already-deployed address returns 409 `already_deployed`; with `RPC_URL` pointed at a dead endpoint, `reserve` returns 503 `onchain_check_unavailable` (fail-closed). Then `POST /x/verification/finalize {token_id}` (200) and `GET /token/{token}` exposes `x_verification`.

- [ ] **Step 5: Commit any fmt/clippy fixups**

```bash
git add -A
git commit -m "chore(x): fmt + clippy for reserve/finalize C1 fix"
```

---

## Self-Review

**1. Spec coverage** (design doc → task):
- §3/§5 six routes → Task 9. §4 DDL (both tables) → Task 1. §4 Redis keys (`x_oauth:state`, `x_pending`) → Task 7 (Redis methods) + Task 6 (structs). §6 `TokenInfo.x_verification` + public types → Task 4. X client (authorize/exchange/get_me/check_follows_me) → Task 5. Redis pending service → Task 7. Postgres controller → Task 8. §7.1 CRITICAL finalize CREATE2 + creator check (with failure tests) → Task 7 (`finalize_checks` tests) + Task 9 (finalize route). §7.2 open-redirect fixed env URL → Task 2 (env) + Task 9 (`failure_redirect`/success uses fixed URL only). §7.3 handle regex → Task 6 (`validate_handle`) + Task 9. §8 error mapping (400 max_followed_by_reached, 200 is_following:false, 410 verification_expired, 403 creator_mismatch) → Task 3 (403/410 variants) + Task 7/9. §8 rate limits (login 3/min, followed-by 5/min via `incr_with_expire`/`check_and_increment`) → Task 9. §8 state one-time GETDEL → Task 7. GET /token join → Task 10. env vars → Task 2. All items map to a task.
- Extra items required by implementation but implied by the design: `AppError::Forbidden/Gone` (needed to emit 403/410), `SaltService::compute_token_address` (needed to reuse CREATE2), `/x/oauth/callback` `api_key_gate` exclusion (needed so X's redirect isn't IP-rate-limited), `x_verification: None` at all 20 TokenInfo sites (forced by the struct change) — all covered.

**2. Placeholder scan:** No TBD/TODO/"handle errors appropriately"/"similar to Task N" left. Every code step contains complete code; every command has an expected result. The one abbreviation ("same two CREATE TABLE statements") in Task 1 Step 4 explicitly points at the verbatim Step 3 block in the same task.

**3. Type consistency:** Verified names line up across tasks — `XFollowedByEntry` (Task 4) reused by Tasks 6/7/8/10; `XPending`/`XOAuthState` (Task 6) consumed by Redis methods (Task 7) with matching `crate::types::x_verification::` paths; `XVerificationService` methods (`start_login`, `complete_callback`, `add_followed_by`, `remove_followed_by`, `get_pending`, `finalize`) called with identical signatures by handlers (Task 9); `XVerificationController::finalize(token_id, account_id, x_user_id, followers_count, &[XFollowedByEntry])` matches the service call; `SaltService::compute_token_address(TokenVersion, B256) -> Result<Address, AppError>` and `compute_create2_address` (now `pub(crate)`) match their callers/tests; `creator_matches`/`address_matches` signatures match both the service and the `finalize_checks` tests; `FinalizeRequest.version` defaults to `V1` mirroring `MineSaltRequest`. `check_and_increment` return arm names (`Exceeded { retry_after, .. }` / `Allowed`) match `rate_limiter.rs`.

**4. C1 remediation (Task 12) coverage — added 2026-07-03 after whole-branch review:**
- **C1 (CRITICAL) — finalize provable-ownership was salt-knowledge only, defeated post-deploy** → Task 12: `reserve` endpoint (creator + CREATE2 + on-chain `eth_getCode` not-deployed + first-writer-wins) at Tasks 12.4/12.6/12.7/12.8; `finalize` simplified to reservation-only at 12.7. Design §7.1-B/§3-bis.
- **I1 (Important) — `FinalizeRequest.version` default V1 → V2 tokens 403** → resolved at Task 12.5: `version` removed from `FinalizeRequest` entirely and moved to `ReserveRequest` with **no serde default (required)**, so CREATE2 uses the correct version and a missing version is a 400, never a silent V1.
- **New security surfaces covered:** fail-closed RPC (12.4, `is_contract_deployed` → 503 on any error), reserve rate limit (12.3 `X_RESERVE_RATE_LIMIT` + 12.8 handler), atomic first-writer-wins (12.6, single-statement upsert + `#[sqlx::test]` conflict test), read-after-write consistency on the finalize gate (12.6 `reservation_owner` uses write pool). Error variants for 409/503 at 12.1.
- **Type consistency (Task 12):** `ReserveRequest { token_id, creator, salt, version }` / `ReserveResponse { ok }` (12.5) consumed by `XVerificationService::reserve` (12.7) and the reserve handler (12.8); `reserve_first_writer(&str,&str)->Result<String>` + `reservation_owner(&str)->Result<Option<String>>` (12.6) called with matching signatures by the service (12.7); `onchain::{code_indicates_deployed(&[u8])->bool, is_contract_deployed(&str)->Result<bool,AppError>}` (12.4) called by `reserve` (12.7); `AppError::{Conflict(String), ServiceUnavailable(String)}` (12.1) emitted by 12.4/12.7. The relocated pure helpers (`creator_matches`/`address_matches`/`canonical_persist_ids`/`compute_token_address`) keep their Task 6/7 signatures — reused by `reserve`, so their existing tests remain valid.
- **Placeholder scan (Task 12):** every code step is complete; every command has an expected result; the one "same statements as Step 2" in 12.2 Step 3 points at the verbatim Step 2 SQL in the same task.
