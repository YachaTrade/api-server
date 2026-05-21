# V2 DEX Router — Positions Endpoint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship `GET /dex/positions/:account_id` — backend for the "My Liquidity Position" card (pair label, my liquidity USD, deposited amounts, TVL, 7d LP-fee APR). Lay down the `dex` router/controller/service scaffold for subsequent endpoints (`/dex/pools/:pool_id`, `/dex/tokens`) to plug into.

**Architecture:** Single SQL query joins `lp_position` × `pool` × `pool_apr` (view) × token-meta (`token` ⊕ `dex_token` ⊕ `quote_token` via COALESCE). APR computed in SQL using gist-confirmed formula `lp_fee_7d_usd / tvl_7d_usd_avg × 365/7 × 100`, NULL-safe via `NULLIF`. Response strings for all NUMERIC fields (wei-precision preservation, matches `vault` controller convention). Redis cache disabled in v1 (position data changes on every mint/burn).

**Tech Stack:** Rust + Axum + sqlx (Postgres) + utoipa (OpenAPI). EIP-55 checksum addresses (`valid_account_id`). BigDecimal for NUMERIC. Follows existing `router::vault` shape exactly.

**Decisions locked in (from upstream brainstorm):**
- APR window: **7d only** (FE mockup shows a single APR value)
- APR null-handling: **JSON `null`** when `tvl_avg=0` / no `pool_apr` row (FE renders "—")
- `lp_share_factor`: NOT exposed (gist-provided view already applies the 0.8 factor inside `lp_fee_*_usd`)
- Position list ordering: by `pool_id` ASC (deterministic; FE can re-sort)
- Closed positions (`balance = 0`): excluded
- USD evaluation: cost basis (frozen at deposit time) for `my_deposited_*_usd`; live `pool.value` for TVL; computed `balance × pool.value / pool.total_supply` for `my_liquidity_usd`

---

## File Structure

**New files (8):**
- `src/router/dex/mod.rs` — Router wiring
- `src/router/dex/path.rs` — Path enum + `as_str` / `docs_str`
- `src/router/dex/handler.rs` — Axum handlers (utoipa annotated)
- `src/services/dex/mod.rs` — Service layer (orchestrator; no caching in v1)
- `src/controllers/dex/mod.rs` — `pub mod position;`
- `src/controllers/dex/position.rs` — Position SQL query + row → DTO
- `src/types/dex/mod.rs` — `pub mod position;`
- `src/types/dex/position.rs` — `LpPositionsResponse`, `LpPositionEntry`, `LpPositionTokenSide`

**Modified files (5):**
- `src/router/mod.rs` — Add `pub mod dex;`
- `src/services/mod.rs` — Add `pub mod dex;`
- `src/controllers/mod.rs` — Add `pub mod dex;`
- `src/types/mod.rs` — Add `pub mod dex;`
- `src/main.rs` — Register router + utoipa paths + ToSchema components + tag

**Test files (2):**
- `src/controllers/dex/position.rs` — bottom `#[cfg(test)] mod tests` for pure-logic helpers (apr formula, balance signs)
- `tests/dex_positions.rs` — Integration test (`#[sqlx::test]` against canonical `./migrations`)

---

## Phase 0: Module Scaffolding (router/path/types skeleton)

### Task 0.1: Create empty `dex` module tree

**Files:**
- Create: `src/router/dex/mod.rs`
- Create: `src/router/dex/path.rs`
- Create: `src/router/dex/handler.rs`
- Create: `src/services/dex/mod.rs`
- Create: `src/controllers/dex/mod.rs`
- Create: `src/controllers/dex/position.rs`
- Create: `src/types/dex/mod.rs`
- Create: `src/types/dex/position.rs`
- Modify: `src/router/mod.rs`
- Modify: `src/services/mod.rs`
- Modify: `src/controllers/mod.rs`
- Modify: `src/types/mod.rs`

- [ ] **Step 1: Create `src/router/dex/path.rs`**

```rust
#[derive(Debug)]
pub enum DexPath {
    GetPositions,
}

impl DexPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            DexPath::GetPositions => "/dex/positions/:account_id",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            DexPath::GetPositions => "/dex/positions/{account_id}",
        }
    }
}
```

- [ ] **Step 2: Create `src/router/dex/mod.rs`**

```rust
pub mod handler;
pub mod path;

use axum::{Router, routing::get};

use path::DexPath;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(
        DexPath::GetPositions.as_str(),
        get(handler::get_positions),
    )
}
```

- [ ] **Step 3: Create stub `src/router/dex/handler.rs`**

```rust
use axum::{
    Json,
    extract::{Path, State},
};
use tracing::instrument;

use super::path::DexPath;
use crate::{
    result::AppJsonResult,
    services::dex::DexService,
    state::AppState,
    types::dex::position::LpPositionsResponse,
    utils::valid_account_id,
};

/// List a wallet's open V2 LP positions (one row per pool with `balance > 0`).
#[utoipa::path(
    get,
    path = DexPath::GetPositions.docs_str(),
    params(
        ("account_id" = String, Path, description = "Wallet address (EIP-55 checksum)")
    ),
    responses(
        (status = 200, description = "Open LP positions for the wallet", body = LpPositionsResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Dex"
)]
#[instrument(skip(state))]
pub async fn get_positions(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> AppJsonResult<LpPositionsResponse> {
    let account_id = valid_account_id(&account_id)
        .ok_or_else(|| crate::result::AppError::BadRequest("Invalid account id".to_string()))?;

    let service = DexService::new(state.postgres.clone());
    let response = service.get_positions(&account_id).await?;

    Ok(Json(response))
}
```

- [ ] **Step 4: Create stub `src/services/dex/mod.rs`**

```rust
use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::dex::position::PositionController,
    db::postgres::PostgresDatabase,
    result::AppError,
    types::dex::position::LpPositionsResponse,
};

pub struct DexService {
    postgres: Arc<PostgresDatabase>,
}

impl DexService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    pub async fn get_positions(
        &self,
        account_id: &str,
    ) -> Result<LpPositionsResponse, AppError> {
        let controller = PositionController::new(self.postgres.clone());
        controller.get_positions(account_id).await.map_err(|err| {
            error!(
                "Failed to get LP positions: account_id={}, error={}",
                account_id, err
            );
            AppError::InternalError(err.to_string())
        })
    }
}
```

- [ ] **Step 5: Create stub `src/controllers/dex/mod.rs`**

```rust
pub mod position;
```

- [ ] **Step 6: Create stub `src/controllers/dex/position.rs` (compiles but returns empty list)**

```rust
use std::sync::Arc;

use anyhow::Result;

use crate::{
    db::postgres::PostgresDatabase,
    types::dex::position::LpPositionsResponse,
};

pub struct PositionController {
    db: Arc<PostgresDatabase>,
}

impl PositionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn get_positions(&self, account_id: &str) -> Result<LpPositionsResponse> {
        // Phase 1 fills this in. Reference `self.db` so the field doesn't get a
        // dead_code warning between scaffold-commit and SQL-commit.
        let _ = &self.db;
        Ok(LpPositionsResponse {
            account_id: account_id.to_string(),
            positions: vec![],
        })
    }
}
```

- [ ] **Step 7: Create stub `src/types/dex/mod.rs`**

```rust
pub mod position;
```

- [ ] **Step 8: Create stub `src/types/dex/position.rs` (full DTO shape — referenced by handler)**

```rust
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Response for `GET /dex/positions/:account_id`.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LpPositionsResponse {
    pub account_id: String,
    pub positions: Vec<LpPositionEntry>,
}

/// One open LP position (single `(account_id, pool_id)` pair with `balance > 0`).
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LpPositionEntry {
    pub pool_id: String,
    /// `"CHOG-WMON"` — composed from `token0.symbol` + `"-"` + `token1.symbol`.
    pub pair_label: String,
    pub token0: LpPositionTokenSide,
    pub token1: LpPositionTokenSide,
    /// Raw LP balance (wei). Matches `lp_position.balance`.
    pub balance: String,
    /// `balance × pool.value / pool.total_supply`. NULL if `pool.total_supply = 0`.
    pub my_liquidity_usd: Option<String>,
    /// `pool.value` — current TVL snapshot in USD as maintained by the indexer.
    pub tvl_usd: String,
    /// 7d LP-net APR as a percentage (e.g. `130.0` = 130%). NULL when undefined
    /// (no `pool_apr` row for this pool, or `tvl_7d_usd_avg = 0`).
    pub apr_pct_7d: Option<String>,
}

/// Per-side token info + cost-basis deposited amount.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct LpPositionTokenSide {
    pub token_id: String,
    pub symbol: String,
    pub decimals: i32,
    pub image_uri: String,
    /// `token0_in - token0_out` (or `token1_in - token1_out`). Raw wei string.
    /// Frozen at deposit time (cost basis), NOT live mark-to-market.
    pub deposited: String,
    /// USD value of `deposited`, frozen at deposit time. Sourced from
    /// `lp_position.token{0,1}_in_usd - token{0,1}_out_usd`.
    pub deposited_usd: String,
}
```

- [ ] **Step 9: Add module declarations to four `mod.rs` files**

`src/router/mod.rs` — append:
```rust
pub mod dex;
```

`src/services/mod.rs` — append:
```rust
pub mod dex;
```

`src/controllers/mod.rs` — append:
```rust
pub mod dex;
```

`src/types/mod.rs` — append:
```rust
pub mod dex;
```

- [ ] **Step 10: Wire router into `src/main.rs`**

In the `use` block at top, add `dex,` to the list of router imports (after `cms,` alphabetically):
```rust
use api_server::{
    ...
    router::{
        self, account, agent, api_key, auth, chester, cms, dex, health, hype, leaderboard, ...
    },
    ...
};
```

In the `#[derive(OpenApi)] paths(...)` block, add (under a new comment header):
```rust
        // ----------------Dex----------------
        router::dex::handler::get_positions,
```

In the `components(schemas(...))` block (search for `types::vault::TokenVaultsResponse`), add:
```rust
            types::dex::position::LpPositionsResponse,
            types::dex::position::LpPositionEntry,
            types::dex::position::LpPositionTokenSide,
```

In the `tags(...)` block (search for `name="Vault"`), add:
```rust
        (name="Dex", description="V2 DEX LP positions, pools, and token list"),
```

In the `Router::new().merge(...)` chain (search for `.merge(vault::router())`), add immediately after:
```rust
        .merge(dex::router())
```

- [ ] **Step 11: Verify everything compiles**

Run: `cargo build`
Expected: clean build. Warnings about unused `db` field in `PositionController` are OK (Phase 1 uses it).

- [ ] **Step 12: Verify endpoint is reachable (stub returns empty list)**

Run dev server (`cargo run`) in another terminal, then:
```bash
curl -s http://localhost:8080/dex/positions/0x0000000000000000000000000000000000000001 | jq
```
Expected:
```json
{"account_id":"0x0000000000000000000000000000000000000001","positions":[]}
```
And bad-checksum input:
```bash
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:8080/dex/positions/notanaddress
```
Expected: `400`.

- [ ] **Step 13: Commit**

```bash
git add src/router/dex src/services/dex src/controllers/dex src/types/dex \
        src/router/mod.rs src/services/mod.rs src/controllers/mod.rs src/types/mod.rs \
        src/main.rs
git commit -m "feat(dex): scaffold /dex router with empty positions stub"
```

---

## Phase 1: Positions Endpoint — Real Query

### Task 1.1: Pure-logic unit tests for APR & balance math

**Files:**
- Modify: `src/controllers/dex/position.rs` (append `#[cfg(test)] mod tests` at bottom + add helper functions)

- [ ] **Step 1: Add pure helper functions above `impl PositionController`**

In `src/controllers/dex/position.rs`, add **above** `pub struct PositionController` (after the imports):

```rust
use sqlx::types::BigDecimal;

/// Multiplier to convert a 7-day return ratio to an annualized percentage.
/// `× 365/7 × 100`.
const APR_7D_PCT_MULTIPLIER: f64 = (365.0 / 7.0) * 100.0;

/// 7-day APR in percent. `None` when undefined (tvl ≤ 0 OR fee is None).
/// Inputs are LP-NET (post-0.8 carve-out) USD values from `pool_apr` view.
pub(crate) fn apr_pct_7d(lp_fee_7d_usd: Option<f64>, tvl_7d_usd_avg: Option<f64>) -> Option<f64> {
    let fee = lp_fee_7d_usd?;
    let tvl = tvl_7d_usd_avg?;
    if tvl <= 0.0 {
        return None;
    }
    Some((fee / tvl) * APR_7D_PCT_MULTIPLIER)
}

/// `my_liquidity_usd = balance × pool.value / pool.total_supply`.
/// `None` when `total_supply <= 0` (defensive — pool has no LP at all).
pub(crate) fn my_liquidity_usd(
    balance: &BigDecimal,
    pool_value_usd: &BigDecimal,
    pool_total_supply: &BigDecimal,
) -> Option<BigDecimal> {
    use bigdecimal::Zero;
    if pool_total_supply.is_zero() || pool_total_supply.sign() == bigdecimal::num_bigint::Sign::Minus {
        return None;
    }
    Some(balance * pool_value_usd / pool_total_supply)
}
```

- [ ] **Step 2: Write failing tests at bottom of file**

Append to `src/controllers/dex/position.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn apr_pct_7d_baseline_130pct() {
        // 130% APR means weekly return of 130 / (365/7) / 100 = 2.49315068...%.
        // Inversely: fee=2.49315068% of TVL → APR = 130.
        let fee = 24.9315068493;
        let tvl = 1000.0;
        let apr = apr_pct_7d(Some(fee), Some(tvl)).unwrap();
        assert!((apr - 130.0).abs() < 1e-6, "got {}", apr);
    }

    #[test]
    fn apr_pct_7d_none_when_fee_missing() {
        assert!(apr_pct_7d(None, Some(1000.0)).is_none());
    }

    #[test]
    fn apr_pct_7d_none_when_tvl_missing() {
        assert!(apr_pct_7d(Some(10.0), None).is_none());
    }

    #[test]
    fn apr_pct_7d_none_when_tvl_zero() {
        assert!(apr_pct_7d(Some(10.0), Some(0.0)).is_none());
    }

    #[test]
    fn apr_pct_7d_none_when_tvl_negative() {
        // pool.value should never go negative, but guard anyway.
        assert!(apr_pct_7d(Some(10.0), Some(-1.0)).is_none());
    }

    #[test]
    fn my_liquidity_usd_basic() {
        let bal = BigDecimal::from(50);
        let value = BigDecimal::from(1000);
        let supply = BigDecimal::from(100);
        // 50 * 1000 / 100 = 500
        let got = my_liquidity_usd(&bal, &value, &supply).unwrap();
        assert_eq!(got, BigDecimal::from(500));
    }

    #[test]
    fn my_liquidity_usd_none_on_zero_supply() {
        let bal = BigDecimal::from(50);
        let value = BigDecimal::from(1000);
        let supply = BigDecimal::from(0);
        assert!(my_liquidity_usd(&bal, &value, &supply).is_none());
    }

    #[test]
    fn my_liquidity_usd_handles_decimal_precision() {
        let bal = BigDecimal::from_str("1234567890123456789").unwrap();
        let value = BigDecimal::from_str("5000.5").unwrap();
        let supply = BigDecimal::from_str("9876543210987654321").unwrap();
        let got = my_liquidity_usd(&bal, &value, &supply).unwrap();
        // Just verify it's positive and finite — exact value tested via direct math.
        assert!(got > BigDecimal::from(0));
    }
}
```

- [ ] **Step 3: Run tests to confirm they pass**

Run: `cargo test --lib position::tests`
Expected: all 7 tests pass.

- [ ] **Step 4: Run with race detector & format check**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: clean (warnings about unused `_` in stub `get_positions` may remain — those go away in Task 1.2).

- [ ] **Step 5: Commit**

```bash
git add src/controllers/dex/position.rs
git commit -m "test(dex): unit tests for APR-7d and my-liquidity formulas"
```

### Task 1.2: Implement the position SQL query

**Files:**
- Modify: `src/controllers/dex/position.rs` (replace stub with real query + row → DTO mapping)

- [ ] **Step 1: Add `measure_postgres!` import + Row struct**

At top of `src/controllers/dex/position.rs`, update imports:
```rust
use std::sync::Arc;

use anyhow::Result;
use sqlx::types::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::dex::position::{LpPositionEntry, LpPositionTokenSide, LpPositionsResponse},
};
```

Between the helper functions and `pub struct PositionController`, add:

```rust
#[derive(Debug, sqlx::FromRow)]
struct PositionRow {
    pool_id: String,
    token0: String,
    token1: String,
    balance: BigDecimal,
    deposited_token0: BigDecimal,
    deposited_token1: BigDecimal,
    deposited_token0_usd: BigDecimal,
    deposited_token1_usd: BigDecimal,
    pool_value_usd: BigDecimal,
    pool_total_supply: BigDecimal,
    token0_symbol: Option<String>,
    token0_decimals: Option<i32>,
    token0_image: Option<String>,
    token1_symbol: Option<String>,
    token1_decimals: Option<i32>,
    token1_image: Option<String>,
    lp_fee_7d_usd: Option<f64>,
    tvl_7d_usd_avg: Option<f64>,
}
```

- [ ] **Step 2: Replace `get_positions` body with the real query**

Replace the existing `impl PositionController { pub async fn get_positions(...) }` with:

```rust
impl PositionController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn get_positions(&self, account_id: &str) -> Result<LpPositionsResponse> {
        let rows = measure_postgres!(
            "dex.get_positions",
            sqlx::query_as::<_, PositionRow>(
                r#"
                SELECT
                    lp.pool_id,
                    p.token0,
                    p.token1,
                    lp.balance,
                    (lp.token0_in     - lp.token0_out)     AS deposited_token0,
                    (lp.token1_in     - lp.token1_out)     AS deposited_token1,
                    (lp.token0_in_usd - lp.token0_out_usd) AS deposited_token0_usd,
                    (lp.token1_in_usd - lp.token1_out_usd) AS deposited_token1_usd,
                    p.value        AS pool_value_usd,
                    p.total_supply AS pool_total_supply,
                    COALESCE(t0.symbol,    dt0.symbol,    qt0.symbol)    AS token0_symbol,
                    COALESCE(t0.decimals,  dt0.decimals,  qt0.decimals)  AS token0_decimals,
                    COALESCE(t0.image_uri, dt0.image_uri, qt0.image_uri) AS token0_image,
                    COALESCE(t1.symbol,    dt1.symbol,    qt1.symbol)    AS token1_symbol,
                    COALESCE(t1.decimals,  dt1.decimals,  qt1.decimals)  AS token1_decimals,
                    COALESCE(t1.image_uri, dt1.image_uri, qt1.image_uri) AS token1_image,
                    par.lp_fee_7d_usd::float8   AS lp_fee_7d_usd,
                    par.tvl_7d_usd_avg::float8  AS tvl_7d_usd_avg
                FROM lp_position lp
                JOIN pool p
                    ON p.pool_id = lp.pool_id
                LEFT JOIN token       t0  ON t0.token_id  = p.token0
                LEFT JOIN dex_token   dt0 ON dt0.token_id = p.token0
                LEFT JOIN quote_token qt0 ON qt0.quote_id = p.token0
                LEFT JOIN token       t1  ON t1.token_id  = p.token1
                LEFT JOIN dex_token   dt1 ON dt1.token_id = p.token1
                LEFT JOIN quote_token qt1 ON qt1.quote_id = p.token1
                LEFT JOIN pool_apr   par  ON par.pool_id  = p.pool_id
                WHERE lp.account_id = $1
                  AND lp.balance > 0
                ORDER BY lp.pool_id
                "#,
            )
            .bind(account_id)
            .fetch_all(self.db.get_read_pool())
            .await?
        );

        let positions = rows.into_iter().map(row_to_entry).collect();
        Ok(LpPositionsResponse {
            account_id: account_id.to_string(),
            positions,
        })
    }
}

fn row_to_entry(r: PositionRow) -> LpPositionEntry {
    let token0_symbol = r.token0_symbol.unwrap_or_default();
    let token1_symbol = r.token1_symbol.unwrap_or_default();
    let pair_label = format!("{}-{}", token0_symbol, token1_symbol);

    let my_liq = my_liquidity_usd(&r.balance, &r.pool_value_usd, &r.pool_total_supply);
    let apr = apr_pct_7d(r.lp_fee_7d_usd, r.tvl_7d_usd_avg);

    LpPositionEntry {
        pool_id: r.pool_id,
        pair_label,
        token0: LpPositionTokenSide {
            token_id: r.token0,
            symbol: token0_symbol,
            decimals: r.token0_decimals.unwrap_or(18),
            image_uri: r.token0_image.unwrap_or_default(),
            deposited: r.deposited_token0.to_string(),
            deposited_usd: r.deposited_token0_usd.to_string(),
        },
        token1: LpPositionTokenSide {
            token_id: r.token1,
            symbol: token1_symbol,
            decimals: r.token1_decimals.unwrap_or(18),
            image_uri: r.token1_image.unwrap_or_default(),
            deposited: r.deposited_token1.to_string(),
            deposited_usd: r.deposited_token1_usd.to_string(),
        },
        balance: r.balance.to_string(),
        my_liquidity_usd: my_liq.map(|v| v.to_string()),
        tvl_usd: r.pool_value_usd.to_string(),
        apr_pct_7d: apr.map(|v| v.to_string()),
    }
}
```

- [ ] **Step 3: Compile**

Run: `cargo build`
Expected: clean. If `measure_postgres!` is not in scope, locate its definition (`grep -r 'macro_rules! measure_postgres' src/`) and import accordingly. If clippy complains about the unused `_` line removed from the stub, that's fine — it's gone now.

- [ ] **Step 4: Re-run unit tests**

Run: `cargo test --lib position::tests`
Expected: 7 tests pass (unit tests should still pass — they don't touch the new query).

- [ ] **Step 5: Commit**

```bash
git add src/controllers/dex/position.rs
git commit -m "feat(dex): implement /dex/positions/:account_id SQL"
```

### Task 1.3: Integration test against real Postgres

**Files:**
- Create: `tests/dex_positions.rs`

- [ ] **Step 1: Verify `[[test]]` discovery convention**

Look at `Cargo.toml` — confirm there's no `[[test]]` block (tests are auto-discovered from `tests/*.rs`). If present, follow its pattern. Otherwise the file naming `tests/dex_positions.rs` works as-is via Cargo defaults.

- [ ] **Step 2: Write the integration test file**

Create `tests/dex_positions.rs`:

```rust
mod common;

use std::sync::Arc;

use api_server::controllers::dex::position::PositionController;
use sqlx::PgPool;

const ACCOUNT: &str = "0x0000000000000000000000000000000000000A11";
const POOL: &str    = "0x9aEB5ZE0000000000000000000000000ef8D88d2"; // placeholder — bytes don't need to be checksum-valid for FK-less inserts
const TOKEN0: &str  = "0x0000000000000000000000000000000000000B01";
const TOKEN1: &str  = "0x0000000000000000000000000000000000000B02";

async fn seed_pool_with_position(pool: &PgPool) {
    // pool — bypass checksum constraints; raw VARCHAR(42)
    sqlx::query(
        r#"INSERT INTO pool (pool_id, token0, token1, reserve0, reserve1, price, volume, value,
                             latest_trade_at, created_at, block_number, tx_hash, total_supply)
           VALUES ($1, $2, $3, 100000, 200000, 2, 0, 1000.61, 0, 0, 1, '0x', 5000)"#,
    )
    .bind(POOL).bind(TOKEN0).bind(TOKEN1)
    .execute(pool).await.unwrap();

    // dex_token rows for both sides
    for (id, sym) in [(TOKEN0, "CHOG"), (TOKEN1, "WMON")] {
        sqlx::query(
            r#"INSERT INTO dex_token (token_id, name, symbol, decimals, image_uri, created_at)
               VALUES ($1, $2, $3, 18, '', 0)"#,
        )
        .bind(id).bind(sym).bind(sym)
        .execute(pool).await.unwrap();
    }

    // lp_position — balance 50, deposited 2500 token0 / 100 token1 (cost basis matches mockup)
    sqlx::query(
        r#"INSERT INTO lp_position (account_id, pool_id, lp_in, lp_out,
                                    token0_in, token0_out, token1_in, token1_out,
                                    token0_in_usd, token0_out_usd, token1_in_usd, token1_out_usd,
                                    created_at, updated_at,
                                    epoch_start_block, epoch_start_tx_index, epoch_start_log_index)
           VALUES ($1, $2, 50, 0,
                   2500, 0, 100, 0,
                   0, 0, 0, 0,
                   0, 0, 0, 0, 0)"#,
    )
    .bind(ACCOUNT).bind(POOL)
    .execute(pool).await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn returns_empty_when_no_positions(pool: PgPool) {
    let controller = PositionController::new(common::test_db(&pool));
    let resp = controller.get_positions(ACCOUNT).await.unwrap();
    assert_eq!(resp.account_id, ACCOUNT);
    assert!(resp.positions.is_empty());
}

#[sqlx::test(migrations = "./migrations")]
async fn returns_open_position_with_correct_fields(pool: PgPool) {
    seed_pool_with_position(&pool).await;
    let controller = PositionController::new(common::test_db(&pool));
    let resp = controller.get_positions(ACCOUNT).await.unwrap();

    assert_eq!(resp.positions.len(), 1);
    let p = &resp.positions[0];
    assert_eq!(p.pool_id, POOL);
    assert_eq!(p.pair_label, "CHOG-WMON");
    assert_eq!(p.token0.symbol, "CHOG");
    assert_eq!(p.token0.deposited, "2500");
    assert_eq!(p.token1.symbol, "WMON");
    assert_eq!(p.token1.deposited, "100");
    assert_eq!(p.balance, "50");
    assert_eq!(p.tvl_usd, "1000.61");
    // my_liquidity_usd = 50 × 1000.61 / 5000 = 10.0061
    let my_liq = p.my_liquidity_usd.as_deref().unwrap();
    assert!(my_liq.starts_with("10.0061"), "got {}", my_liq);
    // No pool_apr row seeded → apr null
    assert!(p.apr_pct_7d.is_none());
}

#[sqlx::test(migrations = "./migrations")]
async fn excludes_closed_positions(pool: PgPool) {
    seed_pool_with_position(&pool).await;
    // Drain the position
    sqlx::query("UPDATE lp_position SET lp_out = lp_in WHERE account_id = $1")
        .bind(ACCOUNT)
        .execute(&pool).await.unwrap();
    let controller = PositionController::new(common::test_db(&pool));
    let resp = controller.get_positions(ACCOUNT).await.unwrap();
    assert!(resp.positions.is_empty(), "closed position should be filtered out");
}
```

- [ ] **Step 3: Verify `tests/common/mod.rs` exposes `test_db`**

```bash
grep -n "pub fn test_db" tests/common/mod.rs
```
Expected: matches line 9 (existing).

- [ ] **Step 4: Make controller module/struct visible to the test crate**

In `src/lib.rs`, verify `pub mod controllers;` exists (line ~3 from initial scan). In `src/controllers/dex/mod.rs`, the `pub mod position;` ensures `position` is reachable as `api_server::controllers::dex::position::PositionController`.

If access fails at test compile time, the helper functions `apr_pct_7d` and `my_liquidity_usd` may need `pub` (already `pub(crate)` — change to `pub` if integration test needs them; this test doesn't, so leave as-is).

- [ ] **Step 5: Run integration tests against a running Postgres**

Prerequisite: `DATABASE_URL` env var pointing at an empty test DB the user can drop+recreate. (Typical: `export DATABASE_URL=postgres://localhost/api_server_test`.)

Run: `cargo test --test dex_positions`
Expected: 3 tests pass.

If migrations fail to apply (e.g., `v2_upgrade_*.sql` non-numbered files trip sqlx — sqlx only picks up `NNNN_*.sql`), confirm sqlx skips them. If sqlx errors on them, move the `v2_upgrade_*.sql` files out of `./migrations` for the test run OR pass an explicit migrator path. Document the workaround in `tests/dex_positions.rs` top-of-file comment.

- [ ] **Step 6: Commit**

```bash
git add tests/dex_positions.rs
git commit -m "test(dex): integration coverage for /dex/positions/:account_id"
```

### Task 1.4: End-to-end manual verification + `/codex review`

- [ ] **Step 1: Boot dev server with V2 schema applied to dev DB**

Confirm the dev DB has `0027_pool_fee_hourly.sql` applied:
```bash
psql "$DATABASE_URL" -c "\d pool_fee_hourly"
psql "$DATABASE_URL" -c "SELECT 1 FROM pool_apr LIMIT 1"
```
Both should succeed (return schema / no error). If not, run the migrations against dev DB first.

Run: `cargo run`

- [ ] **Step 2: Call against a real wallet**

Pick a wallet known to hold V2 LP (ask user or query `SELECT account_id FROM lp_position WHERE balance > 0 LIMIT 1`):
```bash
WALLET=$(psql -At "$DATABASE_URL" -c "SELECT account_id FROM lp_position WHERE balance > 0 LIMIT 1")
curl -s "http://localhost:8080/dex/positions/$WALLET" | jq
```

Verify:
- `pair_label` matches expected pair (e.g. `"CHOG-WMON"`)
- `balance`, `tvl_usd` are numeric strings
- `token0.symbol` / `token1.symbol` are populated (not empty strings)
- `apr_pct_7d` is either a number-string or `null` (NOT a runtime error)
- `my_liquidity_usd` math sanity-checks: ≈ `balance × tvl_usd / pool.total_supply`

- [ ] **Step 3: Run /codex review (mandatory per project CLAUDE.md)**

Invoke `/codex review` against this branch. Apply AUTO-FIX items immediately; for ASK items, confirm with the user.

- [ ] **Step 4: Open PR**

Per project conventions:
- Create a feature branch off `v2` (if not already on one): `git checkout -b feat/dex-positions-endpoint`
- Push & open PR against `v2`
- PR description: link to this plan file + the API team gist + screenshots from `curl` output
- Update `docs/backend/chang.md` (Korean) with the new endpoint entry — submodule PR if `docs/` is a submodule
- After PR merges, move the chang.md entry to `complete.md`

---

## Out of Scope (Separate Plans)

- **`GET /dex/pools/:pool_id`** (image ② Deposit/Withdraw Pool Info panel) — needs reserves, exchange rate, `pool_apr` exposure, fee_config. Plan file: `docs/superpowers/plans/2026-05-XX-v2-dex-router-pools.md`.
- **`GET /dex/tokens`** (image ③ Select Coin list) — needs union of `token` + `dex_token` + `quote_token` with optional `?account=` to attach holder balance. Plan file: `docs/superpowers/plans/2026-05-XX-v2-dex-router-tokens.md`.
- **Redis caching for positions** — deferred until traffic data shows it matters; positions change every mint/burn so TTL would need to be very short.
- **Multi-window APR (24h / 30d)** — add when FE asks for a window toggle.
- **`lp_share_factor` exposure** — defer; the view already pre-applies 0.8 so FE doesn't strictly need it.
