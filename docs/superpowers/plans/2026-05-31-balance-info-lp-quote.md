# BalanceInfo `total_balance` + Wallet∪LP Union Implementation Plan (Revision 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework all 6 `BalanceInfo`-bearing endpoints from "enrich-only" to a **wallet ∪ LP union** (LP-only rows included), add a `total_balance` field (`balance + lp_balance`), recompute `total_count` over the union, and sort the holdings group by `total_balance DESC`.

**Architecture:** Each endpoint's base row-set is expanded to the **union** of: (a) wallet `balance > 0`, (b) V2 `lp_position > 0` (own DB, via `market.pool_id`), (c) V1 LP from Capricorn GraphQL (injected as `unnest($ids, $amts)`). `lp_balance` and `total_balance` are computed **in SQL** (so they can drive `ORDER BY`/pagination). Capricorn lowercase addresses are converted to EIP‑55 checksum **in Rust** before injection → exact index-friendly JOIN, no `LOWER()` in SQL. The Capricorn fetch moves from the handler (post-hoc) into the **service layer** (after the Redis cache check, so cache hits skip the external call); the union response is cached as a whole.

**Sort policy (locked with user, 2026-05-31):**
- Holdings group (`hold-token`, `holder`, agent `holdings`): `ORDER BY total_balance DESC`.
- Provenance group (`created` profile+agent, `gift-fee`): **keep domain sort** — `created` = `created_at DESC`; `gift-fee` = `gift_current_balance DESC NULLS LAST, gift_updated_at DESC NULLS LAST` (NULLS LAST because LP-only rows have no gift-vault row). LP-only rows are still added (full union), `total_balance` is exposed as a field only.

**Tech Stack:** Rust, Axum, sqlx (PostgreSQL), serde, utoipa, `reqwest` (Capricorn client, already built), Redis (`with_cache`/single_flight + per-endpoint response cache, already built).

**Spec:** `docs/superpowers/specs/2026-05-31-balance-info-lp-quote-design.md` — **Revision 2** section is authoritative.

---

## Endpoint → function map (4 functions cover 6 endpoints)

| Function | Endpoints | Base set | Sort |
|---|---|---|---|
| `PositionController::get_hold_token_by_account` | `GET /profile/hold-token/{id}`, `GET /agent/holdings/{id}` | wallet | `total_balance DESC` |
| `PositionController::get_holders_by_token` | `GET /trade/holder/{token}` | wallet holders | `total_balance DESC` |
| `TokenCreatedController::fetch_tokens_created` | `GET /profile/tokens/created/{id}`, `GET /agent/token/created/{id}` | creator tokens | `created_at DESC` |
| `GiftFeeController::fetch_gift_fee_tokens` | `GET /profile/gift-fee/{id}` | gift receiver tokens | gift balance DESC NULLS LAST |

Agent and profile variants call the **same service**, so fixing the service+controller fixes both.

## Reuse vs rewrite (current 9 commits)

- **Reuse as-is:** `BalanceInfo.lp_balance`/`quote_price` fields, `CapricornClient` (fetch/cached/aggregate/fail-to-empty + tests), `current_share` (`pub(crate)`, still used by `dex/position.rs`), `migrations-test/0021_lp_position.sql`.
- **Rewrite:** the 4 controller queries (union + SQL `lp_balance`/`total_balance` + ORDER BY), the 4 `*total_count*` functions (union distinct), the 3 services (inject `CapricornClient`, fetch+convert before controller), the handlers (drop post-hoc enrichment).
- **Add:** `total_balance` struct field; Capricorn union helpers (`lp_amounts_by_token`, `lp_amounts_by_owner`, cap); union integration tests.

## Stated assumptions (verify against prod, do not silently rely on)

1. **Unit consistency:** `balance`, `pool.reserve0/1`, `pool.total_supply`, and Capricorn `amountHuman` are all stored at the **same decimal-adjusted scale**. V2 formula `lp.balance * reserveN / total_supply` yields the same units as `balance` (spec §91); V1 `amountHuman` is human-scaled and assumed equal to that scale (user live-measured `0x3500…` ≈ 12M tokens, human-readable). `total_balance = balance + lp_balance` is therefore a like-units sum. If prod shows a 10^decimals mismatch for V1, escalate before shipping — do not paper over it.
2. **Redis response cache now holds the union** (incl. Capricorn V1). On Capricorn failure the union simply omits V1 LP rows ("fail-to-empty") for that cache window. This is acceptable per spec §126/§129.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `src/types/common/info.rs` | `BalanceInfo` definition + serialization test | add `total_balance` field |
| `src/services/capricorn/mod.rs` | Capricorn client + pure aggregation helpers | add `lp_amounts_by_token`, `lp_amounts_by_owner`, `CAPRICORN_UNION_CAP` |
| `src/controllers/trading/position.rs` | hold-token + holder queries & counts | union rewrite, SQL `lp_balance`/`total_balance`, count rewrite |
| `src/controllers/token/create.rs` | created query & count | union rewrite (keep `created_at DESC`) |
| `src/controllers/token/gift_fee.rs` | gift-fee query & count | union rewrite (keep gift sort, NULLS LAST) |
| `src/services/trading/position.rs` | PositionService | inject `CapricornClient`, fetch+convert, pass V1 tuples |
| `src/services/token/create.rs` | TokenCreatedService | same |
| `src/services/token/gift_fee.rs` | GiftFeeService | same |
| `src/router/profile/handler.rs` | profile handlers | remove post-hoc enrichment, pass `state.capricorn` to services |
| `src/router/agent/handler.rs` | agent handlers | same |
| `src/router/trade/handler.rs` | holder handler | pass `state.capricorn` to `PositionService::new` |
| `docs/V2_API_CHANGES.md`, `docs/backend/chang.md`, `branches/feat-balance-info-lp-quote.md` | docs | record `total_balance` + union behavior |

**Task dependency:** 1 (struct) → 2 (helpers) → 3 (hold-token) → 4 (holder) → 5 (created) → 6 (gift-fee) → 7 (docs/regression). Each task ends compiling + green.

**Shared SQL building blocks (used verbatim in Tasks 3–6):**

`v1_lp` CTE — V1 LP injected from Capricorn (checksummed ids + amounts):
```sql
WITH v1_lp AS (
    SELECT token_id, amt
    FROM unnest($V1IDS::varchar[], $V1AMTS::numeric[]) AS u(token_id, amt)
)
```

`lp_balance` SQL expression (V2 inline formula + V1 injected; a token is only ever one or the other):
```sql
(
  COALESCE(
    CASE WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
         WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
         ELSE 0 END, 0)
  + COALESCE(v1.amt, 0)
)
```
`total_balance` = `COALESCE(b.balance, 0) + <lp_balance expression>`.

---

## Task 1: `total_balance` field on `BalanceInfo`

`lp_balance` and `quote_price` already exist (prior commits). Add `total_balance` and populate it at every construction site.

**Files:**
- Modify: `src/types/common/info.rs:166-181` (struct), `:218-232` (test)
- Modify (construction sites, set in later tasks too): `src/controllers/trading/position.rs` (holder ~:159, hold-token ~:373), `src/controllers/token/create.rs` (~:335), `src/controllers/token/gift_fee.rs` (~:339)

- [ ] **Step 1: Update the serialization test (failing)**

In `src/types/common/info.rs`, replace the existing test body so it asserts `total_balance`:

```rust
    #[test]
    fn balance_info_serializes_lp_balance_quote_price_and_total_balance() {
        let b = BalanceInfo {
            balance: "100".into(),
            lp_balance: "5".into(),
            total_balance: "105".into(),
            token_price: "2".into(),
            native_price: "3".into(),
            quote_price: "3".into(),
            created_at: 0,
        };
        let v = serde_json::to_value(&b).unwrap();
        assert_eq!(v["lp_balance"], "5");
        assert_eq!(v["total_balance"], "105");
        assert_eq!(v["quote_price"], "3");
        assert_eq!(v["quote_price"], v["native_price"]); // dual-field
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --lib types::common::info`
Expected: compile error — no field `total_balance` on `BalanceInfo`.

- [ ] **Step 3: Add the field to the struct**

In `src/types/common/info.rs`, add `total_balance` right after `lp_balance`:

```rust
pub struct BalanceInfo {
    /// Wallet-held token balance
    pub balance: String,
    /// LP-locked amount of this token in the same units as `balance`. "0" when none.
    pub lp_balance: String,
    /// balance + lp_balance, in the same units. Sort key for holdings lists.
    pub total_balance: String,
    /// Token/USD price
    pub token_price: String,
    /// quote/USD price (legacy alias; 실제로는 quote_id 기준 USD)
    pub native_price: String,
    /// quote/USD price (canonical). 현재 native_price와 동일 값
    pub quote_price: String,
    // holding period (지갑 balance 기준; LP-only 행은 0)
    pub created_at: i64,
}
```

- [ ] **Step 4: Populate `total_balance` at all 4 current construction sites (temporary, refined in Tasks 3–6)**

The union tasks will source `total_balance` from a SQL column. For now keep the crate compiling by computing it from the existing strings. Add a tiny helper in `src/utils/mod.rs`:

```rust
/// Sum two decimal strings (token amounts), returning a normalized plain string.
/// Falls back to "0" on parse failure (never panics).
pub fn add_token_amounts(a: &str, b: &str) -> String {
    use bigdecimal::BigDecimal;
    use std::str::FromStr;
    let x = BigDecimal::from_str(a).unwrap_or_else(|_| BigDecimal::from(0));
    let y = BigDecimal::from_str(b).unwrap_or_else(|_| BigDecimal::from(0));
    (x + y).normalized().to_plain_string()
}
```

Then in each `BalanceInfo { ... }` literal, add `total_balance` immediately after `lp_balance`. Use a `let` binding for the two strings so they aren't computed twice. Example (hold-token, position.rs ~:373):

```rust
balance_info: {
    let balance = row.balance.normalized().to_plain_string();
    let lp_balance = match (&row.lp_raw, &row.lp_reserve, &row.lp_total_supply) {
        (Some(b), Some(r), Some(ts)) => crate::controllers::dex::position::current_share(b, r, ts)
            .map(|v| v.normalized().to_plain_string())
            .unwrap_or_else(|| "0".to_string()),
        _ => "0".to_string(),
    };
    let total_balance = crate::utils::add_token_amounts(&balance, &lp_balance);
    BalanceInfo {
        total_balance,
        balance,
        lp_balance,
        token_price: row.token_price.normalized().to_plain_string(),
        native_price: row.native_price.normalized().to_plain_string(),
        quote_price: row.native_price.normalized().to_plain_string(),
        created_at: row.balance_created_at,
    }
},
```

Apply the same `total_balance: crate::utils::add_token_amounts(&balance, &lp_balance)` pattern at the holder site (position.rs ~:159), create.rs (~:335), and gift_fee.rs (~:339). (Tasks 3–6 will replace these with the SQL `total_balance` column.)

- [ ] **Step 5: Run tests + build**

Run: `cargo test --lib types::common::info && cargo build`
Expected: PASS, full crate compiles.

- [ ] **Step 6: Commit**

```bash
git add src/types/common/info.rs src/utils/mod.rs src/controllers/trading/position.rs src/controllers/token/create.rs src/controllers/token/gift_fee.rs
git commit -m "feat(balance): add total_balance field to BalanceInfo (computed balance+lp_balance)"
```

---

## Task 2: Capricorn union helpers

For the union we need, for account endpoints, **every token the owner has LP in** (not just a candidate list), and for the holder endpoint, **every owner with LP in a token**. Output addresses are EIP‑55 checksummed (so SQL joins exactly). Plus a cap.

**Files:**
- Modify: `src/services/capricorn/mod.rs` (add helpers + tests below the existing ones)

- [ ] **Step 1: Write failing unit tests**

Add to the `#[cfg(test)] mod tests` in `src/services/capricorn/mod.rs`:

```rust
    #[test]
    fn lp_amounts_by_token_sums_both_sides_and_checksums() {
        // owner holds the same nadfun token on token0 of two positions
        let lower = "0x000000000000000000000000000000000000bb01"; // -> checksum 0x..Bb01
        let positions = vec![
            CapricornPosition { owner: "0xa".into(), token0: lower.into(), token1: "0xwmon".into(),
                amount0_human: "3".into(), amount1_human: "0".into() },
            CapricornPosition { owner: "0xa".into(), token0: lower.into(), token1: "0xwmon".into(),
                amount0_human: "4.5".into(), amount1_human: "0".into() },
        ];
        let out = lp_amounts_by_token(&positions);
        // key is EIP-55 checksum of `lower`, amount summed 3 + 4.5 = 7.5.
        // NOTE: use valid_account_id (pure EIP-55), NOT valid_token_id — the latter also
        // enforces VANITY_ADDRESS_SUFFIX, which would drop legacy V1 token addresses.
        let checksum = crate::utils::valid_account_id(lower).unwrap();
        let found = out.iter().find(|(id, _)| id == &checksum).expect("token present");
        assert_eq!(found.1.normalized().to_plain_string(), "7.5");
        // the WMON side ("0xwmon") is an invalid address -> dropped by checksum
        assert!(out.iter().all(|(id, _)| id == &checksum));
    }

    #[test]
    fn lp_amounts_by_owner_sums_per_owner_for_token() {
        let token = "0x000000000000000000000000000000000000bb01";
        let positions = vec![
            CapricornPosition { owner: "0x000000000000000000000000000000000000aa01".into(),
                token0: token.into(), token1: "0xw".into(), amount0_human: "2".into(), amount1_human: "0".into() },
            CapricornPosition { owner: "0x000000000000000000000000000000000000aa01".into(),
                token0: token.into(), token1: "0xw".into(), amount0_human: "5".into(), amount1_human: "0".into() },
        ];
        let out = lp_amounts_by_owner(&positions, token);
        let owner_cs = crate::utils::valid_account_id("0x000000000000000000000000000000000000aa01").unwrap();
        let found = out.iter().find(|(o, _)| o == &owner_cs).expect("owner present");
        assert_eq!(found.1.normalized().to_plain_string(), "7"); // 2 + 5
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib services::capricorn::tests::lp_amounts`
Expected: FAIL — `lp_amounts_by_token` / `lp_amounts_by_owner` not defined.

- [ ] **Step 3: Implement the helpers + cap**

Add to `src/services/capricorn/mod.rs` (top-level, near `aggregate_lp_by_token`):

```rust
use bigdecimal::BigDecimal;

/// Hard cap on injected LP rows per request. Above this, the caller falls back to
/// enrich-only (no row injection) to keep union sort/pagination bounded (spec §214).
pub const CAPRICORN_UNION_CAP: usize = 2000;

fn parse_amt(s: &str) -> BigDecimal {
    use std::str::FromStr;
    BigDecimal::from_str(s).unwrap_or_else(|_| BigDecimal::from(0))
}

/// For account endpoints: every (nadfun-token-address, summed lp amount) the owner holds LP in.
/// Both pool sides are emitted; addresses that fail EIP-55 parsing (e.g. WMON pseudo-addrs) are
/// dropped — the SQL JOIN to our `token` table filters non-nadfun tokens anyway.
/// Output keys are EIP-55 checksum.
/// Uses `valid_account_id` (pure EIP-55), NOT `valid_token_id`: the latter also enforces
/// VANITY_ADDRESS_SUFFIX and would silently drop legacy V1 token addresses that lack it.
pub fn lp_amounts_by_token(positions: &[CapricornPosition]) -> Vec<(String, BigDecimal)> {
    use std::collections::HashMap;
    let mut acc: HashMap<String, BigDecimal> = HashMap::new();
    for p in positions {
        for (addr, amt) in [(&p.token0, &p.amount0_human), (&p.token1, &p.amount1_human)] {
            if let Some(checksum) = crate::utils::valid_account_id(addr) {
                *acc.entry(checksum).or_insert_with(|| BigDecimal::from(0)) += parse_amt(amt);
            }
        }
    }
    acc.into_iter().collect()
}

/// For the holder endpoint: every (owner, summed lp amount of `token_id`) for one token.
/// Owner addresses are EIP-55 checksummed; invalid owners are dropped.
pub fn lp_amounts_by_owner(positions: &[CapricornPosition], token_id: &str) -> Vec<(String, BigDecimal)> {
    use std::collections::HashMap;
    let mut acc: HashMap<String, BigDecimal> = HashMap::new();
    for p in positions {
        if let Some(amt) = p.amount_for_token(token_id) {
            if let Some(owner_cs) = crate::utils::valid_account_id(&p.owner) {
                *acc.entry(owner_cs).or_insert_with(|| BigDecimal::from(0)) += parse_amt(&amt);
            }
        }
    }
    acc.into_iter().collect()
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test --lib services::capricorn`
Expected: PASS (new + existing).

- [ ] **Step 5: Commit**

```bash
git add src/services/capricorn/mod.rs
git commit -m "feat(capricorn): union helpers lp_amounts_by_token/by_owner + cap (checksummed)"
```

---

## Task 3: hold-token union (profile hold-token + agent holdings)

Rewrite `get_hold_token_by_account` to the wallet∪LP union, compute `lp_balance`/`total_balance` in SQL, sort `total_balance DESC`, recompute `total_count`. Inject `CapricornClient` into `PositionService`; fetch+convert on Redis miss. Remove handler post-hoc enrichment for hold-token and agent holdings.

**Files:**
- Modify: `src/controllers/trading/position.rs` (`HoldTokenRow`, `get_hold_token_by_account`, `get_total_count_by_hold_token`)
- Modify: `src/services/trading/position.rs` (constructor + `get_hold_token_by_account`)
- Modify: `src/router/profile/handler.rs` (`get_hold_token`), `src/router/agent/handler.rs` (`get_holdings`), `src/router/trade/handler.rs` (`PositionService::new` call site — pass capricorn even though holder union lands in Task 4)
- Test: `src/controllers/trading/position.rs` `#[cfg(test)]`

- [ ] **Step 1: Write failing integration tests (controller-level, direct V1 injection)**

Add to the existing `#[cfg(test)] mod tests` in `position.rs`. The controller method gains a `v1_lp: &[(String, BigDecimal)]` param (Step 3). These tests pass it directly (no HTTP):

```rust
    #[sqlx::test(migrations = "./migrations-test")]
    async fn hold_token_sorts_by_total_balance_desc(pool: PgPool) {
        seed_v2_dex(&pool).await;          // ACCOUNT: balance 500, lp_balance 1000 -> total 1500
        seed_account2_balance(&pool).await; // unrelated holder
        let ctrl = make_controller(pool);
        let p = PaginationParams { page: 1, limit: 10, direction: "DESC".to_string() };
        let resp = ctrl.get_hold_token_by_account(ACCOUNT, &p, &[]).await.unwrap();
        let tok = &resp.tokens[0];
        assert_eq!(tok.balance_info.balance, "500");
        assert_eq!(tok.balance_info.lp_balance, "1000");
        assert_eq!(tok.balance_info.total_balance, "1500");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn hold_token_includes_lp_only_v2(pool: PgPool) {
        seed_v2_dex(&pool).await;
        // wallet balance 0 but lp_position present -> LP-only row must still appear
        sqlx::query("DELETE FROM balance WHERE account_id=$1 AND token_id=$2")
            .bind(ACCOUNT).bind(TOKEN_ID).execute(&pool).await.unwrap();
        let ctrl = make_controller(pool);
        let p = PaginationParams { page: 1, limit: 10, direction: "DESC".to_string() };
        let resp = ctrl.get_hold_token_by_account(ACCOUNT, &p, &[]).await.unwrap();
        assert_eq!(resp.tokens.len(), 1, "LP-only token must be present");
        assert_eq!(resp.tokens[0].balance_info.balance, "0");
        assert_eq!(resp.tokens[0].balance_info.lp_balance, "1000");
        assert_eq!(resp.tokens[0].balance_info.total_balance, "1000");
        assert_eq!(resp.total_count, 1);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn hold_token_includes_lp_only_v1_injected(pool: PgPool) {
        seed_v2_dex(&pool).await; // reuse token row; mark it V1, drop pool/lp so only injected V1 counts
        sqlx::query("UPDATE token SET version='V1' WHERE token_id=$1").bind(TOKEN_ID).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM lp_position WHERE account_id=$1").bind(ACCOUNT).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM balance WHERE account_id=$1 AND token_id=$2").bind(ACCOUNT).bind(TOKEN_ID).execute(&pool).await.unwrap();
        let ctrl = make_controller(pool);
        let p = PaginationParams { page: 1, limit: 10, direction: "DESC".to_string() };
        let v1 = vec![(TOKEN_ID.to_string(), BigDecimal::from(42))];
        let resp = ctrl.get_hold_token_by_account(ACCOUNT, &p, &v1).await.unwrap();
        assert_eq!(resp.tokens.len(), 1);
        assert_eq!(resp.tokens[0].balance_info.lp_balance, "42");
        assert_eq!(resp.tokens[0].balance_info.total_balance, "42");
    }
```

Also update the three existing `get_hold_token_by_account(ACCOUNT, &pagination)` calls in this module to pass `&[]` as the new third arg.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --test ... ` → use: `cargo test --lib controllers::trading::position`
Expected: compile error — arity mismatch (method now takes `v1_lp`).

- [ ] **Step 3: Rewrite `get_hold_token_by_account`**

In `src/controllers/trading/position.rs`:

(a) Change the signature:
```rust
    pub async fn get_hold_token_by_account(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        v1_lp: &[(String, bigdecimal::BigDecimal)],
    ) -> Result<HoldTokenResponse> {
```

(b) Before the query, split the tuples and pass as arrays:
```rust
        let offset = (pagination.page - 1) * pagination.limit;
        let v1_ids: Vec<String> = v1_lp.iter().map(|(id, _)| id.clone()).collect();
        let v1_amts: Vec<bigdecimal::BigDecimal> = v1_lp.iter().map(|(_, a)| a.clone()).collect();
```

(c) In `HoldTokenRow`, replace `lp_raw/lp_reserve/lp_total_supply` with:
```rust
            balance: BigDecimal,
            balance_created_at: i64,
            lp_balance: BigDecimal,
            total_balance: BigDecimal,
```
(keep all other fields).

(d) Replace the query string with the union (note `$4`=v1_ids, `$5`=v1_amts; `LIMIT $2 OFFSET $3` unchanged):
```sql
                WITH v1_lp AS (
                    SELECT token_id, amt
                    FROM unnest($4::varchar[], $5::numeric[]) AS u(token_id, amt)
                ),
                held AS (
                    SELECT token_id FROM balance WHERE account_id = $1 AND balance > 0
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                    UNION
                    SELECT token_id FROM v1_lp
                )
                SELECT
                    t.token_id, t.name, t.symbol, t.image_uri, t.description,
                    t.twitter, t.telegram, t.website, t.is_graduated, t.is_nsfw, t.is_cto,
                    t.version, t.created_at, t.creator,
                    COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                    a.bio as creator_bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    COALESCE(b.balance, 0) as balance,
                    COALESCE(b.created_at, 0) as balance_created_at,
                    (m.price * COALESCE(lp.price, 0)) as token_price,
                    COALESCE(lp.price, 0) as native_price,
                    m.market_type,
                    COALESCE(m.pool_id, '') as market_id,
                    COALESCE(m.quote_id, '') as quote_id,
                    m.price,
                    (m.price * COALESCE(lp.price, 0)) as price_usd,
                    t.total_supply,
                    COALESCE(m.reserve_quote, 0) as reserve_quote,
                    COALESCE(m.reserve_token, 0) as reserve_token,
                    m.volume, m.ath_price, m.ath_price_quote,
                    COALESCE(qt.name, '') as quote_name,
                    COALESCE(qt.symbol, '') as quote_symbol,
                    COALESCE(qt.decimals, 18) as quote_decimals,
                    COALESCE(qt.image_uri, '') as quote_image_uri,
                    t.token_holder_count as holder_count,
                    fc.creator_fee_rate, fc.curve_protocol_fee_rate, fc.dex_protocol_fee_rate,
                    (
                      COALESCE(CASE WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                    WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                    ELSE 0 END, 0)
                      + COALESCE(v1.amt, 0)
                    ) AS lp_balance,
                    (
                      COALESCE(b.balance, 0)
                      + COALESCE(CASE WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                      WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                      ELSE 0 END, 0)
                      + COALESCE(v1.amt, 0)
                    ) AS total_balance
                FROM held h
                JOIN token t  ON t.token_id = h.token_id
                JOIN market m ON m.token_id = t.token_id
                JOIN quote_token qt ON m.quote_id = qt.quote_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                LEFT JOIN fee_config fc ON t.token_id = fc.token_id
                LEFT JOIN balance b ON b.token_id = t.token_id AND b.account_id = $1
                LEFT JOIN pool ON pool.pool_id = m.pool_id
                LEFT JOIN lp_position lp_pos ON lp_pos.pool_id = pool.pool_id AND lp_pos.account_id = $1
                LEFT JOIN v1_lp v1 ON v1.token_id = t.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price FROM price p WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC LIMIT 1
                ) lp ON true
                ORDER BY total_balance DESC
                LIMIT $2 OFFSET $3
```
Bindings: `.bind(account_id).bind(pagination.limit).bind(offset).bind(&v1_ids).bind(&v1_amts)`.

(e) `total_count`: call `self.get_total_count_by_hold_token(account_id, &v1_ids).await?` (new signature in Step 4). Keep the `if records.is_empty() { 0 }` guard.

(f) In the row→`TokenWithBalanceInfo` map, set the BalanceInfo block from SQL columns (drop the `current_share` match):
```rust
                    balance_info: BalanceInfo {
                        balance: row.balance.normalized().to_plain_string(),
                        lp_balance: row.lp_balance.normalized().to_plain_string(),
                        total_balance: row.total_balance.normalized().to_plain_string(),
                        token_price: row.token_price.normalized().to_plain_string(),
                        native_price: row.native_price.normalized().to_plain_string(),
                        quote_price: row.native_price.normalized().to_plain_string(),
                        created_at: row.balance_created_at,
                    },
```

- [ ] **Step 4: Rewrite `get_total_count_by_hold_token` to count the union**

```rust
    pub async fn get_total_count_by_hold_token(
        &self,
        account_id: &str,
        v1_ids: &[String],
    ) -> Result<i64> {
        let count = measure_postgres!(
            "position.get_total_count_by_hold_token",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(DISTINCT h.token_id)::bigint as count
                FROM (
                    SELECT token_id FROM balance WHERE account_id = $1 AND balance > 0
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                    UNION
                    SELECT token_id FROM unnest($2::varchar[]) AS u(token_id)
                ) h
                JOIN token t  ON t.token_id = h.token_id
                JOIN market m ON m.token_id = t.token_id
                "#,
            )
            .bind(account_id)
            .bind(v1_ids)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get hold token count: {}", err))?;
        Ok(count.count)
    }
```

- [ ] **Step 5: Run controller tests**

Run: `cargo test --lib controllers::trading::position`
Expected: PASS (new union tests + existing V2 lp tests; existing tests pass `&[]`).

- [ ] **Step 6: Inject Capricorn into `PositionService` + fetch on miss**

In `src/services/trading/position.rs`:

(a) Add field + constructor arg:
```rust
use crate::services::capricorn::{CapricornClient, lp_amounts_by_token, CAPRICORN_UNION_CAP};

pub struct PositionService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
    capricorn: Arc<CapricornClient>,
}

impl PositionService {
    pub fn new(
        postgres: Arc<PostgresDatabase>,
        redis: Arc<RedisDatabase>,
        capricorn: Arc<CapricornClient>,
    ) -> Self {
        Self { postgres, redis, capricorn }
    }
```

(b) In `get_hold_token_by_account`, after the Redis cache check (miss path), fetch+convert and pass to the controller:
```rust
        let positions = self.capricorn.cached_fetch_by_owner(account_id).await;
        let mut v1_lp = lp_amounts_by_token(&positions);
        if v1_lp.len() > CAPRICORN_UNION_CAP {
            tracing::warn!("capricorn owner LP rows {} exceed cap; V1 LP omitted for this response", v1_lp.len());
            v1_lp.clear(); // fall back to wallet∪V2 only
        }
        let controller = PositionController::new(self.postgres.clone());
        let response = controller
            .get_hold_token_by_account(account_id, pagination, &v1_lp)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;
```
(Keep the existing Redis get/set wrapping around this. The full union response is what gets cached.)

- [ ] **Step 7: Update all `PositionService::new` call sites + drop hold-token/holdings post-hoc enrichment**

Find every call site: `grep -rn "PositionService::new" src/`. For each, add `state.capricorn.clone()` as the third arg. Then:

- `src/router/profile/handler.rs::get_hold_token` — delete the `// Enrich V1 lp_balance ...` block (lines ~95-110) and the now-unused `aggregate_lp_by_token` / `TokenVersion` imports **iff** no longer used in the file (check; created/gift blocks are removed in Tasks 5/6, so keep the import until then or remove per-file at the end — simplest: leave imports, remove in Task 7 cleanup). `response` no longer needs `mut` if nothing else mutates it — change `let mut response` to `let response`.
- `src/router/agent/handler.rs::get_holdings` — delete its enrichment block (lines ~245-258), `let mut response` → `let response`.
- `src/router/trade/handler.rs` — update the `PositionService::new(...)` call to pass `state.capricorn.clone()` (holder union lands in Task 4; passing it now keeps compile green).

- [ ] **Step 8: Build + targeted test**

Run: `cargo build && cargo test --lib controllers::trading::position`
Expected: PASS, full crate compiles.

- [ ] **Step 9: Commit**

```bash
git add src/controllers/trading/position.rs src/services/trading/position.rs src/router/profile/handler.rs src/router/agent/handler.rs src/router/trade/handler.rs
git commit -m "feat(balance): hold-token wallet∪LP union + total_balance DESC sort + union count"
```

---

## Task 4: holder union (trade/holder)

Rewrite `get_holders_by_token` to wallet∪LP union over **owners** of one token, SQL `lp_balance`/`total_balance`, `total_balance DESC`, union `total_count`, with cap → enrich-only fallback.

**Files:**
- Modify: `src/controllers/trading/position.rs` (`TokenHolderRow`, `get_holders_by_token`, `get_total_count_by_token_holder`)
- Modify: `src/services/trading/position.rs` (`get_holders_by_token`)
- Test: `position.rs` `#[cfg(test)]`

- [ ] **Step 1: Write failing tests**

`get_holders_by_token` gains `v1_lp: &[(String /*owner checksum*/, BigDecimal)]`. Add:

```rust
    #[sqlx::test(migrations = "./migrations-test")]
    async fn holder_includes_lp_only_owner_v2(pool: PgPool) {
        seed_v2_dex(&pool).await;          // ACCOUNT: balance 500 + lp 1000 -> total 1500
        // ACCOUNT3: lp only (no balance)
        let acct3 = "0x000000000000000000000000000000000000Aa03";
        sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,'h3','','') ON CONFLICT DO NOTHING").bind(acct3).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO lp_position (account_id,pool_id,lp_in,lp_out,token0_in,token0_out,token1_in,token1_out,token0_in_usd,token0_out_usd,token1_in_usd,token1_out_usd,created_at,updated_at,epoch_start_block,epoch_start_tx_index,epoch_start_log_index) VALUES ($1,$2,50,0,0,0,0,0,0,0,0,0,0,0,0,0,0) ON CONFLICT DO NOTHING"#).bind(acct3).bind(POOL_ID).execute(&pool).await.unwrap();
        let ctrl = make_controller(pool);
        let p = PaginationParams { page: 1, limit: 10, direction: "DESC".to_string() };
        let resp = ctrl.get_holders_by_token(TOKEN_ID, &p, &[]).await.unwrap();
        // ACCOUNT (1500) first, ACCOUNT3 lp-only (50*10000/1000=500) present with balance 0
        let acct3_row = resp.holders.iter().find(|h| h.account_info.account_id == acct3).expect("lp-only holder present");
        assert_eq!(acct3_row.balance_info.balance, "0");
        assert_eq!(acct3_row.balance_info.lp_balance, "500");
        assert_eq!(acct3_row.balance_info.total_balance, "500");
        assert_eq!(resp.total_count, 2);
        assert_eq!(resp.holders[0].account_info.account_id, ACCOUNT); // total_balance DESC
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn holder_includes_lp_only_owner_v1_injected(pool: PgPool) {
        seed_v2_dex(&pool).await;
        sqlx::query("UPDATE token SET version='V1' WHERE token_id=$1").bind(TOKEN_ID).execute(&pool).await.unwrap();
        let acct4 = "0x000000000000000000000000000000000000Aa04";
        sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,'h4','','') ON CONFLICT DO NOTHING").bind(acct4).execute(&pool).await.unwrap();
        let ctrl = make_controller(pool);
        let p = PaginationParams { page: 1, limit: 10, direction: "DESC".to_string() };
        let v1 = vec![(acct4.to_string(), BigDecimal::from(777))];
        let resp = ctrl.get_holders_by_token(TOKEN_ID, &p, &v1).await.unwrap();
        let row = resp.holders.iter().find(|h| h.account_info.account_id == acct4).expect("v1 lp-only holder present");
        assert_eq!(row.balance_info.lp_balance, "777");
        assert_eq!(row.balance_info.total_balance, "777");
    }
```

Update existing holder tests (`holder_lp_balance_v2_dex_computed_correctly`, etc.) to pass `&[]` as the third arg.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib controllers::trading::position::tests::holder_includes`
Expected: compile error (arity).

- [ ] **Step 3: Rewrite `get_holders_by_token`**

(a) Signature: add `v1_lp: &[(String, bigdecimal::BigDecimal)]`. Split into `v1_owners`/`v1_amts` like Task 3 Step 3(b).

(b) `TokenHolderRow`: replace `lp_raw/lp_reserve/lp_total_supply` with `lp_balance: BigDecimal, total_balance: BigDecimal` (keep `balance, token_price, native_price, balance_created_at, account_id, nickname, bio, image_uri`). Add `account_created` is not needed.

(c) Query (account-side union; `$1`=token_id, `$2`=offset, `$3`=limit, `$4`=v1_owners, `$5`=v1_amts):
```sql
                WITH v1_lp AS (
                    SELECT account_id, amt
                    FROM unnest($4::varchar[], $5::numeric[]) AS u(account_id, amt)
                ),
                holders AS (
                    SELECT account_id FROM balance WHERE token_id = $1 AND balance > 0
                    UNION
                    SELECT lp.account_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE m.token_id = $1 AND lp.balance > 0
                    UNION
                    SELECT account_id FROM v1_lp
                )
                SELECT
                    COALESCE(b.balance, 0) as balance,
                    (m.price * COALESCE(lp.price, 0)) as token_price,
                    COALESCE(lp.price, 0) as native_price,
                    COALESCE(b.created_at, 0) as balance_created_at,
                    a.account_id,
                    COALESCE(ax.x_handle, a.nickname) as nickname,
                    a.bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as image_uri,
                    (
                      COALESCE(CASE WHEN pool.token0 = m.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                    WHEN pool.token1 = m.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                    ELSE 0 END, 0)
                      + COALESCE(v1.amt, 0)
                    ) AS lp_balance,
                    (
                      COALESCE(b.balance, 0)
                      + COALESCE(CASE WHEN pool.token0 = m.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                      WHEN pool.token1 = m.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                      ELSE 0 END, 0)
                      + COALESCE(v1.amt, 0)
                    ) AS total_balance
                FROM holders h
                JOIN account a ON a.account_id = h.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                JOIN market m ON m.token_id = $1
                LEFT JOIN balance b ON b.token_id = $1 AND b.account_id = h.account_id
                LEFT JOIN pool ON pool.pool_id = m.pool_id
                LEFT JOIN lp_position lp_pos ON lp_pos.pool_id = pool.pool_id AND lp_pos.account_id = h.account_id
                LEFT JOIN v1_lp v1 ON v1.account_id = h.account_id
                LEFT JOIN LATERAL (
                    SELECT p.price FROM price p WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC LIMIT 1
                ) lp ON true
                ORDER BY total_balance DESC, a.account_id ASC
                OFFSET $2 LIMIT $3
```
Bindings: `.bind(token_id).bind(offset).bind(pagination.limit).bind(&v1_owners).bind(&v1_amts)`. (The `a.account_id ASC` tiebreaker makes pagination deterministic on `total_balance` ties.)

(d) `total_count`: `self.get_total_count_by_token_holder(token_id, &v1_owners).await?` (Step 4). Keep `if records.is_empty() { 0 }`.

(e) Row→`TokenHolder` map: set `balance_info` from SQL columns:
```rust
                balance_info: BalanceInfo {
                    balance: row.balance.normalized().to_plain_string(),
                    lp_balance: row.lp_balance.normalized().to_plain_string(),
                    total_balance: row.total_balance.normalized().to_plain_string(),
                    token_price: row.token_price.normalized().to_plain_string(),
                    native_price: row.native_price.normalized().to_plain_string(),
                    quote_price: row.native_price.normalized().to_plain_string(),
                    created_at: row.balance_created_at,
                },
```

- [ ] **Step 4: Rewrite `get_total_count_by_token_holder` to union distinct**

The current count uses `token.token_holder_count`. Replace with a union distinct over the token's wallet holders + V2 LP holders + injected V1 owners:
```rust
    pub async fn get_total_count_by_token_holder(
        &self,
        token_id: &str,
        v1_owners: &[String],
    ) -> Result<i64> {
        let count = measure_postgres!(
            "position.get_total_count_by_token_holder",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(DISTINCT h.account_id)::bigint as count
                FROM (
                    SELECT account_id FROM balance WHERE token_id = $1 AND balance > 0
                    UNION
                    SELECT lp.account_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE m.token_id = $1 AND lp.balance > 0
                    UNION
                    SELECT account_id FROM unnest($2::varchar[]) AS u(account_id)
                ) h
                JOIN account a ON a.account_id = h.account_id
                "#,
            )
            .bind(token_id)
            .bind(v1_owners)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to get token holder count: {}", err))?;
        Ok(count.count)
    }
```
> If `fetch_token_holder_count`/`get_total_count_by_token_holder` is called elsewhere (e.g. token metadata), `grep -rn "get_total_count_by_token_holder\|fetch_token_holder_count" src/` and update those call sites to pass `&[]` (they want the wallet+V2 holder count without external V1 injection). Keep the old `fetch_token_holder_count` private helper only if still referenced; otherwise remove.

- [ ] **Step 5: Run controller tests**

Run: `cargo test --lib controllers::trading::position`
Expected: PASS.

- [ ] **Step 6: Service `get_holders_by_token` — fetch by token + cap**

In `src/services/trading/position.rs::get_holders_by_token`, on the Redis miss path:
```rust
        let positions = self.capricorn.cached_fetch_by_token(token_id).await;
        let mut v1_lp = crate::services::capricorn::lp_amounts_by_owner(&positions, token_id);
        if v1_lp.len() > crate::services::capricorn::CAPRICORN_UNION_CAP {
            tracing::warn!("capricorn token LP owners {} exceed cap; V1 LP omitted for this response", v1_lp.len());
            v1_lp.clear();
        }
        let controller = PositionController::new(self.postgres.clone());
        let response = controller
            .get_holders_by_token(token_id, pagination, &v1_lp)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;
```
(Keep the existing Redis get/set around it.)

- [ ] **Step 7: Build + test**

Run: `cargo build && cargo test --lib controllers::trading::position`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/controllers/trading/position.rs src/services/trading/position.rs
git commit -m "feat(balance): holder wallet∪LP union + total_balance DESC + union count + cap"
```

---

## Task 5: created union (profile created + agent created)

Union the creator's tokens with V2/V1 LP tokens; **keep `created_at DESC`**; expose `total_balance`; union `total_count`.

**Files:**
- Modify: `src/controllers/token/create.rs` (`TokenCreatedRow`, `fetch_tokens_created`, `fetch_total_count`)
- Modify: `src/services/token/create.rs` (constructor + method)
- Modify: `src/router/profile/handler.rs::get_token_created`, `src/router/agent/handler.rs::get_tokens_created`
- Test: new `tests/created_union.rs` (or controller `#[cfg(test)]` if a test module exists; create.rs currently has none → add `#[cfg(test)]` mod with `#[sqlx::test]`)

- [ ] **Step 1: Write failing test**

Add a `#[cfg(test)] mod tests` to `create.rs` (mirror the position.rs fixture style; seed a creator token + an unrelated V2 LP token for the same account). Minimal assertion focus: an LP-only token (account is NOT creator, has lp_position) appears, and `total_balance` is present:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bigdecimal::BigDecimal;
    use sqlx::PgPool;
    use std::sync::Arc;
    use crate::types::common::pagination::PaginationParams;

    const ACCOUNT: &str = "0x000000000000000000000000000000000000Aa01";
    const OTHER:   &str = "0x000000000000000000000000000000000000Aa09"; // creator of the LP-only token
    const CREATED_TOKEN: &str = "0x000000000000000000000000000000000000Bb01";
    const LP_TOKEN:      &str = "0x000000000000000000000000000000000000Bb09";
    const POOL_ID:  &str = "0x000000000000000000000000000000000000Cc09";
    const QUOTE_ID: &str = "0x3bd359C1119dA7Da1D913D1C4D2B7c461115433A";

    fn ctrl(pool: PgPool) -> TokenCreatedController {
        TokenCreatedController::new(Arc::new(crate::db::postgres::PostgresDatabase { write_pool: pool.clone(), read_pool: pool }))
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn created_union_includes_lp_only_token(pool: PgPool) {
        // accounts
        for (a, n) in [(ACCOUNT, "me"), (OTHER, "other")] {
            sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,$2,'','') ON CONFLICT DO NOTHING").bind(a).bind(n).execute(&pool).await.unwrap();
        }
        // a token I created
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply,version) VALUES ($1,'Mine','MINE','',$2,NULL,false,false,false,100,'0xh',1000000,'V2') ON CONFLICT DO NOTHING"#).bind(CREATED_TOKEN).bind(ACCOUNT).execute(&pool).await.unwrap();
        // a token someone else created, that I provide LP to
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply,version) VALUES ($1,'Lp','LP','',$2,NULL,false,false,false,200,'0xh',1000000,'V2') ON CONFLICT DO NOTHING"#).bind(LP_TOKEN).bind(OTHER).execute(&pool).await.unwrap();
        for t in [CREATED_TOKEN, LP_TOKEN] {
            sqlx::query("INSERT INTO swap_count (token_id,count,buy_count,sell_count) VALUES ($1,0,0,0) ON CONFLICT DO NOTHING").bind(t).execute(&pool).await.unwrap();
        }
        // markets (CREATED_TOKEN: no pool needed; LP_TOKEN: pool with my lp_position)
        sqlx::query(r#"INSERT INTO market (market_type,token_id,pool_id,reserve_token,reserve_quote,price,quote_id,latest_trade_at,created_at,volume,ath_price,ath_price_quote) VALUES ('V2_DEX',$1,NULL,0,0,1,$2,0,0,0,0,0) ON CONFLICT (token_id) DO NOTHING"#).bind(CREATED_TOKEN).bind(QUOTE_ID).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO pool (pool_id,token0,token1,reserve0,reserve1,price,volume,value,latest_trade_at,created_at,block_number,tx_hash,total_supply) VALUES ($1,$2,$3,10000,20000,1,0,0,0,0,1,'0xtx',1000) ON CONFLICT DO NOTHING"#).bind(POOL_ID).bind(LP_TOKEN).bind("0x000000000000000000000000000000000000Bb0a").execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO market (market_type,token_id,pool_id,reserve_token,reserve_quote,price,quote_id,latest_trade_at,created_at,volume,ath_price,ath_price_quote) VALUES ('V2_DEX',$1,$2,10000,20000,1,$3,0,0,0,0,0) ON CONFLICT (token_id) DO NOTHING"#).bind(LP_TOKEN).bind(POOL_ID).bind(QUOTE_ID).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO lp_position (account_id,pool_id,lp_in,lp_out,token0_in,token0_out,token1_in,token1_out,token0_in_usd,token0_out_usd,token1_in_usd,token1_out_usd,created_at,updated_at,epoch_start_block,epoch_start_tx_index,epoch_start_log_index) VALUES ($1,$2,100,0,0,0,0,0,0,0,0,0,0,0,0,0,0) ON CONFLICT DO NOTHING"#).bind(ACCOUNT).bind(POOL_ID).execute(&pool).await.unwrap();

        let p = PaginationParams { page: 1, limit: 10, direction: "DESC".to_string() };
        let resp = ctrl(pool).get_tokens_created(ACCOUNT, &p, &[]).await.unwrap();
        let ids: Vec<&str> = resp.tokens.iter().map(|t| t.token_info.token_id.as_str()).collect();
        assert!(ids.contains(&CREATED_TOKEN), "created token present");
        assert!(ids.contains(&LP_TOKEN), "LP-only token present in created union");
        // created_at DESC: LP_TOKEN(200) before CREATED_TOKEN(100)
        assert_eq!(ids[0], LP_TOKEN);
        let lp_row = resp.tokens.iter().find(|t| t.token_info.token_id == LP_TOKEN).unwrap();
        assert_eq!(lp_row.balance_info.lp_balance, "1000"); // 100*10000/1000
        assert_eq!(lp_row.balance_info.total_balance, "1000");
        assert_eq!(resp.total_count, 2);
    }
}
```

> Naming: keep the public method name `get_tokens_created` and extend its signature to `get_tokens_created(account_id, pagination, v1_lp)`; the private worker stays `fetch_tokens_created(account_id, pagination, v1_lp)`. Use this exact name in controller, service, and test — no `_with_v1` variant.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib controllers::token::create`
Expected: compile error (method/arity).

- [ ] **Step 3: Rewrite `fetch_tokens_created` (and its public wrapper `get_tokens_created`)**

(a) `get_tokens_created` + `fetch_tokens_created` signatures gain `v1_lp: &[(String, bigdecimal::BigDecimal)]`. Split into `v1_ids`/`v1_amts`.

(b) `TokenCreatedRow`: remove `lp_raw/lp_reserve/lp_total_supply`; add `lp_balance: BigDecimal, total_balance: BigDecimal`.

(c) Replace the CTE/query. `paged_tokens` becomes the union, sort unchanged (`created_at DESC`). `$1`=account, `$2`=limit, `$3`=offset, `$4`=v1_ids, `$5`=v1_amts:
```sql
                WITH v1_lp AS (
                    SELECT token_id, amt FROM unnest($4::varchar[], $5::numeric[]) AS u(token_id, amt)
                ),
                member_ids AS (
                    SELECT token_id FROM token WHERE creator = $1
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                    UNION
                    SELECT token_id FROM v1_lp
                ),
                paged_tokens AS (
                    SELECT t.* FROM token t
                    WHERE t.token_id IN (SELECT token_id FROM member_ids)
                      AND EXISTS (SELECT 1 FROM market mk WHERE mk.token_id = t.token_id)
                    ORDER BY t.created_at DESC, t.token_id ASC
                    LIMIT $2 OFFSET $3
                ),
                claimed_totals AS (
                    SELECT token_id, SUM(amount) as claimed_amount
                    FROM creator_treasury_claim_history
                    WHERE account_id = $1
                    GROUP BY token_id
                )
                SELECT
                    t.token_id, t.name as token_name, t.symbol as token_symbol,
                    t.image_uri as token_image_uri, t.description as token_description,
                    t.twitter as token_twitter, t.telegram as token_telegram, t.website as token_website,
                    t.is_graduated, t.is_nsfw, t.is_cto, t.version,
                    t.created_at as token_created_at, t.creator, t.token_holder_count as holder_count,
                    COALESCE(ax.x_handle, a.nickname) as creator_nickname,
                    a.bio as creator_bio, COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    m.market_type, COALESCE(m.pool_id, '') as market_id, COALESCE(m.quote_id, '') as quote_id,
                    (m.price * COALESCE(lp.price, 0)) as token_price, COALESCE(lp.price, 0) as native_price,
                    m.price, (m.price * COALESCE(lp.price, 0)) as price_usd, t.total_supply,
                    COALESCE(m.reserve_quote, 0) as reserve_quote, COALESCE(m.reserve_token, 0) as reserve_token,
                    m.volume, m.ath_price, m.ath_price_quote,
                    COALESCE(qt.name, '') as quote_name, COALESCE(qt.symbol, '') as quote_symbol,
                    COALESCE(qt.decimals, 18) as quote_decimals, COALESCE(qt.image_uri, '') as quote_image_uri,
                    fc.creator_fee_rate, fc.curve_protocol_fee_rate, fc.dex_protocol_fee_rate,
                    COALESCE(b.balance, 0) as balance, COALESCE(b.created_at, 0) as balance_created_at,
                    COALESCE(cr.amount, 0) as reward_amount, COALESCE(ctch.claimed_amount, 0) as reward_claimed_amount,
                    COALESCE(cr.proof, ARRAY[]::TEXT[]) as reward_proof, cr.status as reward_status,
                    v2cfv.current_balance as v2_current_balance, v2cfv.total_claimed as v2_total_claimed,
                    (
                      COALESCE(CASE WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                    WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                    ELSE 0 END, 0) + COALESCE(v1.amt, 0)
                    ) AS lp_balance,
                    (
                      COALESCE(b.balance, 0)
                      + COALESCE(CASE WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                      WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                      ELSE 0 END, 0) + COALESCE(v1.amt, 0)
                    ) AS total_balance
                FROM paged_tokens t
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                JOIN market m ON t.token_id = m.token_id
                JOIN quote_token qt ON m.quote_id = qt.quote_id
                LEFT JOIN fee_config fc ON t.token_id = fc.token_id
                LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
                LEFT JOIN creator_reward cr ON t.token_id = cr.token_id AND cr.account_id = $1
                LEFT JOIN claimed_totals ctch ON t.token_id = ctch.token_id
                LEFT JOIN v2_creator_fee_vault_stats v2cfv ON t.token_id = v2cfv.token_id
                LEFT JOIN pool ON pool.pool_id = m.pool_id
                LEFT JOIN lp_position lp_pos ON lp_pos.pool_id = pool.pool_id AND lp_pos.account_id = $1
                LEFT JOIN v1_lp v1 ON v1.token_id = t.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price FROM price p WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC LIMIT 1
                ) lp ON true
                ORDER BY t.created_at DESC, t.token_id ASC
```
Bindings unchanged order + `.bind(&v1_ids).bind(&v1_amts)`.

(d) Row→`TokenCreatedInfo` map: set `balance_info` from SQL columns (`balance`, `lp_balance`, `total_balance`, prices, `created_at: row.balance_created_at`). `reward_info` unchanged.

(e) `total_count`: `self.fetch_total_count(account_id, &v1_ids).await?`.

- [ ] **Step 4: Rewrite `fetch_total_count` (+ public `get_total_count`)**

```rust
    async fn fetch_total_count(&self, account_id: &str, v1_ids: &[String]) -> Result<i64> {
        let count = measure_postgres!(
            "token_created.fetch_total_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(DISTINCT mm.token_id)::bigint as count
                FROM (
                    SELECT token_id FROM token WHERE creator = $1
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                    UNION
                    SELECT token_id FROM unnest($2::varchar[]) AS u(token_id)
                ) mm
                JOIN token t  ON t.token_id = mm.token_id
                JOIN market m ON m.token_id = t.token_id
                "#,
            )
            .bind(account_id)
            .bind(v1_ids)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch tokens created count: {}", err))?;
        Ok(count.count)
    }
```
> The response's union count comes from `fetch_tokens_created` calling `self.fetch_total_count(account_id, &v1_ids)` with the real ids. The public cached `get_total_count(account_id)` has **no callers** (verified by grep) and its controller is DB-only (no Capricorn) — so do NOT thread `v1_ids` into it or its `cache_key!`. Just update its internal `controller.fetch_total_count(&account_id)` call to `controller.fetch_total_count(&account_id, &[])` so it compiles (creator∪V2 count; harmless since unused), keeping its cache key as-is. If `CountRow` isn't already defined in this module, define `#[derive(sqlx::FromRow)] struct CountRow { count: i64 }`.

- [ ] **Step 5: Run controller tests**

Run: `cargo test --lib controllers::token::create`
Expected: PASS.

- [ ] **Step 6: Service injection + handlers**

- `src/services/token/create.rs`: add `capricorn: Arc<CapricornClient>` field + constructor arg; in `get_tokens_created`, on Redis miss fetch `cached_fetch_by_owner(account_id)` → `lp_amounts_by_token` → cap → pass `&v1_lp` to controller.
- `src/router/profile/handler.rs::get_token_created` + `src/router/agent/handler.rs::get_tokens_created`: pass `state.capricorn.clone()` to `TokenCreatedService::new`; delete the post-hoc enrichment blocks; `let mut response` → `let response`.
- `grep -rn "TokenCreatedService::new" src/` and update any other call sites.

- [ ] **Step 7: Build + test**

Run: `cargo build && cargo test --lib controllers::token::create`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/controllers/token/create.rs src/services/token/create.rs src/router/profile/handler.rs src/router/agent/handler.rs
git commit -m "feat(balance): created wallet∪LP union (keep created_at DESC) + total_balance + union count"
```

---

## Task 6: gift-fee union

Union gift-receiver tokens with V2/V1 LP tokens; **keep gift sort with NULLS LAST**; expose `total_balance`; union `total_count`.

**Files:**
- Modify: `src/controllers/token/gift_fee.rs` (`Row`, `fetch_gift_fee_tokens`, `fetch_total_count`)
- Modify: `src/services/token/gift_fee.rs`
- Modify: `src/router/profile/handler.rs::get_gift_fee`
- Test: `gift_fee.rs` `#[cfg(test)]`

- [ ] **Step 1: Write failing test**

Add a `#[cfg(test)] mod tests` mirroring Task 5's seeding, but the base set is `v2_gift_vault_stats.receiver = $1`. Seed: one gift token (account is receiver, `v2_gift_vault_stats` row with `current_balance`), one LP-only token (account has lp_position, no gift row). Assert: both appear; gift token sorts first (gift balance DESC NULLS LAST puts LP-only NULL last); LP-only has `total_balance` set; `total_count == 2`.

```rust
    #[sqlx::test(migrations = "./migrations-test")]
    async fn gift_fee_union_lp_only_sorts_last(pool: PgPool) {
        // ... seed accounts, GIFT_TOKEN (receiver=ACCOUNT, v2_gift_vault_stats.current_balance=999),
        //     LP_TOKEN (account has lp_position, no gift row, pool 100*10000/1000=1000) ...
        let p = PaginationParams { page: 1, limit: 10, direction: "DESC".to_string() };
        let resp = ctrl(pool).get_gift_fee_tokens(ACCOUNT, &p, &[]).await.unwrap();
        let ids: Vec<&str> = resp.tokens.iter().map(|t| t.token_info.token_id.as_str()).collect();
        assert_eq!(ids[0], GIFT_TOKEN, "gift token (non-null gift balance) sorts first");
        assert!(ids.contains(&LP_TOKEN), "LP-only token present");
        let lp_row = resp.tokens.iter().find(|t| t.token_info.token_id == LP_TOKEN).unwrap();
        assert_eq!(lp_row.balance_info.lp_balance, "1000");
        assert_eq!(resp.total_count, 2);
    }
```
> Check `v2_gift_vault_stats` columns in `migrations-test/` before seeding (receiver, token_id, current_balance, total_claimed, updated_at). Add a `swap_count` row per token (FK), as in Task 5.

- [ ] **Step 2: Run to verify failure**

Run: `cargo test --lib controllers::token::gift_fee`
Expected: compile error (arity).

- [ ] **Step 3: Rewrite `fetch_gift_fee_tokens`**

(a) `get_gift_fee_tokens` + `fetch_gift_fee_tokens` gain `v1_lp: &[(String, BigDecimal)]`; split to `v1_ids`/`v1_amts`.

(b) `Row`: remove `lp_raw/lp_reserve/lp_total_supply`; add `lp_balance: BigDecimal, total_balance: BigDecimal`.

(c) Query — `paged_tokens` unions gift tokens with LP tokens; gift fields LEFT JOINed from `v2_gift_vault_stats`; sort `NULLS LAST`. `$1`=account, `$2`=limit, `$3`=offset, `$4`=v1_ids, `$5`=v1_amts:
```sql
                WITH v1_lp AS (
                    SELECT token_id, amt FROM unnest($4::varchar[], $5::numeric[]) AS u(token_id, amt)
                ),
                member_ids AS (
                    SELECT token_id FROM v2_gift_vault_stats WHERE receiver = $1
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                    UNION
                    SELECT token_id FROM v1_lp
                ),
                paged_tokens AS (
                    SELECT mi.token_id,
                           g.current_balance AS gift_current_balance,
                           g.total_claimed   AS gift_total_claimed,
                           g.updated_at      AS gift_updated_at
                    FROM member_ids mi
                    JOIN token tk  ON tk.token_id = mi.token_id
                    JOIN market mk ON mk.token_id = mi.token_id
                    LEFT JOIN v2_gift_vault_stats g ON g.token_id = mi.token_id AND g.receiver = $1
                    ORDER BY g.current_balance DESC NULLS LAST, g.updated_at DESC NULLS LAST, mi.token_id ASC
                    LIMIT $2 OFFSET $3
                ),
                claimed_totals AS (
                    SELECT token_id, SUM(amount) as claimed_amount
                    FROM creator_treasury_claim_history WHERE account_id = $1 GROUP BY token_id
                )
                SELECT
                    t.token_id, t.name as token_name, t.symbol as token_symbol, t.image_uri as token_image_uri,
                    t.description as token_description, t.twitter as token_twitter, t.telegram as token_telegram, t.website as token_website,
                    t.is_graduated, t.is_nsfw, t.is_cto, t.version, t.created_at as token_created_at, t.creator,
                    t.token_holder_count as holder_count,
                    COALESCE(ax.x_handle, a.nickname) as creator_nickname, a.bio as creator_bio,
                    COALESCE(ax.x_image_uri, a.image_uri) as creator_image_uri,
                    m.market_type, COALESCE(m.pool_id, '') as market_id, COALESCE(m.quote_id, '') as quote_id,
                    (m.price * COALESCE(lp.price, 0)) as token_price, COALESCE(lp.price, 0) as native_price,
                    m.price, (m.price * COALESCE(lp.price, 0)) as price_usd, t.total_supply,
                    COALESCE(m.reserve_quote, 0) as reserve_quote, COALESCE(m.reserve_token, 0) as reserve_token,
                    m.volume, m.ath_price, m.ath_price_quote,
                    COALESCE(qt.name, '') as quote_name, COALESCE(qt.symbol, '') as quote_symbol,
                    COALESCE(qt.decimals, 18) as quote_decimals, COALESCE(qt.image_uri, '') as quote_image_uri,
                    fc.creator_fee_rate, fc.curve_protocol_fee_rate, fc.dex_protocol_fee_rate,
                    COALESCE(b.balance, 0) as balance, COALESCE(b.created_at, 0) as balance_created_at,
                    COALESCE(cr.amount, 0) as reward_amount, COALESCE(ctch.claimed_amount, 0) as reward_claimed_amount,
                    COALESCE(cr.proof, ARRAY[]::TEXT[]) as reward_proof, cr.status as reward_status,
                    pg.gift_current_balance as v2_current_balance, pg.gift_total_claimed as v2_total_claimed,
                    (
                      COALESCE(CASE WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                    WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                    ELSE 0 END, 0) + COALESCE(v1.amt, 0)
                    ) AS lp_balance,
                    (
                      COALESCE(b.balance, 0)
                      + COALESCE(CASE WHEN pool.token0 = t.token_id THEN lp_pos.balance * pool.reserve0 / NULLIF(pool.total_supply, 0)
                                      WHEN pool.token1 = t.token_id THEN lp_pos.balance * pool.reserve1 / NULLIF(pool.total_supply, 0)
                                      ELSE 0 END, 0) + COALESCE(v1.amt, 0)
                    ) AS total_balance
                FROM paged_tokens pg
                JOIN token t ON t.token_id = pg.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                JOIN market m ON t.token_id = m.token_id
                JOIN quote_token qt ON m.quote_id = qt.quote_id
                LEFT JOIN fee_config fc ON t.token_id = fc.token_id
                LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
                LEFT JOIN creator_reward cr ON t.token_id = cr.token_id AND cr.account_id = $1
                LEFT JOIN claimed_totals ctch ON t.token_id = ctch.token_id
                LEFT JOIN pool ON pool.pool_id = m.pool_id
                LEFT JOIN lp_position lp_pos ON lp_pos.pool_id = pool.pool_id AND lp_pos.account_id = $1
                LEFT JOIN v1_lp v1 ON v1.token_id = t.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price FROM price p WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC LIMIT 1
                ) lp ON true
                ORDER BY pg.gift_current_balance DESC NULLS LAST, pg.gift_updated_at DESC NULLS LAST, pg.token_id ASC
```
Bindings: `.bind(account_id).bind(pagination.limit).bind(offset).bind(&v1_ids).bind(&v1_amts)`.

(d) Row→`TokenCreatedInfo` map: `balance_info` from SQL columns. `reward_info`/gift fields unchanged.

(e) `total_count`: `self.fetch_total_count(account_id, &v1_ids).await?`.

- [ ] **Step 4: Rewrite `fetch_total_count` for gift union**

```rust
    async fn fetch_total_count(&self, account_id: &str, v1_ids: &[String]) -> Result<i64> {
        let count = measure_postgres!(
            "gift_fee.fetch_total_count",
            sqlx::query_as::<_, CountRow>(
                r#"
                SELECT COUNT(DISTINCT mm.token_id)::bigint as count
                FROM (
                    SELECT token_id FROM v2_gift_vault_stats WHERE receiver = $1
                    UNION
                    SELECT m.token_id FROM lp_position lp
                        JOIN market m ON m.pool_id = lp.pool_id
                        WHERE lp.account_id = $1 AND lp.balance > 0
                    UNION
                    SELECT token_id FROM unnest($2::varchar[]) AS u(token_id)
                ) mm
                JOIN token t  ON t.token_id = mm.token_id
                JOIN market m ON m.token_id = t.token_id
                "#,
            )
            .bind(account_id)
            .bind(v1_ids)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch gift fee count: {}", err))?;
        Ok(count.count)
    }
```
> Same as Task 5: `fetch_gift_fee_tokens` calls `self.fetch_total_count(account_id, &v1_ids)`. The public cached `get_total_count` is uncalled — just update its internal `controller.fetch_total_count(&account_id, &[])` to compile; keep its cache key. Do NOT thread `v1_ids` into the public method/key.

- [ ] **Step 5: Run controller tests**

Run: `cargo test --lib controllers::token::gift_fee`
Expected: PASS.

- [ ] **Step 6: Service injection + handler**

- `src/services/token/gift_fee.rs`: add `capricorn` field + ctor arg; on Redis miss `cached_fetch_by_owner` → `lp_amounts_by_token` → cap → pass to controller.
- `src/router/profile/handler.rs::get_gift_fee`: pass `state.capricorn.clone()`; delete enrichment block; `let mut response` → `let response`. Then remove now-unused imports (`aggregate_lp_by_token`, `TokenVersion`) from `profile/handler.rs` and `agent/handler.rs` if nothing else references them (`grep` within each file first).
- `grep -rn "GiftFeeService::new" src/` and update any other sites.

- [ ] **Step 7: Build + test**

Run: `cargo build && cargo test --lib controllers::token::gift_fee`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/controllers/token/gift_fee.rs src/services/token/gift_fee.rs src/router/profile/handler.rs src/router/agent/handler.rs
git commit -m "feat(balance): gift-fee wallet∪LP union (gift sort NULLS LAST) + total_balance + union count"
```

---

## Task 7: full regression + docs

**Files:**
- Modify: `docs/V2_API_CHANGES.md`, `docs/backend/chang.md`
- Create: `branches/feat-balance-info-lp-quote.md`

- [ ] **Step 1: Full test suite (race)**

Run: `cargo test --all 2>&1 | tail -40` then `cargo test -- --include-ignored` if any sqlx tests are gated. Use a running Postgres for `#[sqlx::test]`.
Expected: all PASS. If a pre-existing unrelated test fails, note it (do not fix out of scope).

- [ ] **Step 2: fmt + clippy**

Run: `cargo fmt && cargo clippy --all-targets -- -D warnings`
Expected: clean. Fix only warnings introduced by this change.
> Cleanup (YAGNI / remove latent traps):
> - After Tasks 3–6 source `total_balance` from SQL, the `add_token_amounts` helper added in Task 1 is no longer called. `grep -rn "add_token_amounts" src/` — if the only hit is its definition, remove it (and its tests).
> - The public cached `get_total_count` on `TokenCreatedController` (create.rs) and `GiftFeeController` (gift_fee.rs) now passes `&[]` to `fetch_total_count`, so it would under-count V1-LP-only tokens. Both have ZERO callers (`grep -rn "\.get_total_count(" src/` shows none for these — distinct from the `*_by_hold_token`/`*_by_token_holder` variants which ARE used). Remove the dead public `get_total_count` (+ its `with_cache`/`cache_key!`) from create.rs and gift_fee.rs to eliminate the trap. Re-confirm no callers before deleting; if any caller exists, instead doc-comment the V1 limitation rather than delete.

- [ ] **Step 3: Update `docs/V2_API_CHANGES.md`**

Add a section (Korean per CLAUDE.md docs rule): `BalanceInfo`에 `total_balance` 추가; 6개 엔드포인트가 지갑∪LP 합집합(LP-only 포함)으로 전환; `total_count` 합집합 distinct 재계산; 정렬 — 홀딩(hold-token/holder/holdings)=`total_balance DESC`, created=`created_at DESC`, gift-fee=gift balance DESC NULLS LAST; V1 LP는 Capricorn 주입, 실패 시 LP-only 제외(fail-to-empty); cap=2000. Note FE: created/gift에 creator/receiver 아닌 LP-only 토큰이 등장할 수 있음(의도); LP-only 행은 `created_at=0`.

- [ ] **Step 4: Update `docs/backend/chang.md`**

Per CLAUDE.md changelog lifecycle, add the in-progress entry; it moves to `complete.md` with date + hash after merge.

- [ ] **Step 5: Create `branches/feat-balance-info-lp-quote.md`**

Concept doc: Purpose (BalanceInfo lp_balance/quote_price/total_balance + wallet∪LP union), Changes (this plan's tasks + key commits), Outcome (filled at PR).

- [ ] **Step 6: Commit**

```bash
git add docs/V2_API_CHANGES.md docs/backend/chang.md branches/feat-balance-info-lp-quote.md
git commit -m "docs(balance): total_balance + wallet∪LP union behavior, branch concept doc"
```

---

## Verification (completion criteria)

- [ ] `cargo test --all` PASS, `cargo build` succeeds, `cargo clippy -D warnings` clean.
- [ ] All 6 endpoints return `total_balance` (= `balance + lp_balance`), `lp_balance`, `quote_price` (= `native_price`).
- [ ] LP-only rows appear in all 6 (V2 via `lp_position`, V1 via Capricorn injection) — verified by integration tests for hold-token, holder, created, gift-fee.
- [ ] Sort: hold-token/holder = `total_balance DESC`; created = `created_at DESC`; gift-fee = gift balance DESC with LP-only (NULL) last.
- [ ] `total_count` = union distinct count (matches the unioned page set's joins).
- [ ] Capricorn failure ⇒ V1 LP rows omitted, response 200, no error propagation (existing client tests + service fail-to-empty).
- [ ] Capricorn rows > 2000 ⇒ enrich-only fallback, logged.
- [ ] No `LOWER()` added to any SQL (checksum done in Rust).
- [ ] Handler post-hoc enrichment blocks removed; `CapricornClient` injected via all 3 service constructors; agent + profile variants both covered (shared services).

## Unresolved / follow-ups (spec §9 + Rev2)

- Unit-scale assumption #1 — confirm against prod for a real V1 token before/at PR.
- `CAPRICORN_GRAPHQL_URL` production value (env).
- Redis response-cache TTL now bounds Capricorn freshness — verify the existing TTL on these keys is acceptable (`grep` the `set_*` cache methods); reduce if stale V1 LP is a problem.
- `created_at=0` + `total_balance>0` (LP-only rows) — FE display agreement.
- API versioning interaction (`2026-04-11-api-url-versioning`): new required `total_balance` field vs strict V1 clients.
