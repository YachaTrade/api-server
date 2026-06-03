# Terminal API V2 Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Terminal API (`/pair`, `/events`) serve V2_CURVE / V2_DEX tokens correctly by replacing every hardcoded `WMON` quote assumption with the per-token `market.quote_id`, while keeping all V1 responses byte-identical.

**Architecture:** Keep the existing data sources (`swap` / `mint` / `burn` unified tables — confirmed by the observer as the canonical per-token event sink). Add `quote_id` (and per-event `market_type`, plus `fee_config`) to the controller SQL, extract the V2-aware routing decisions into four pure free functions, and rewrite the service `get_pair` + event converters to use them. No migration (the `quote_id` column already exists).

**Tech Stack:** Rust, Axum, sqlx (Postgres), bigdecimal, utoipa.

**Design reference:** `docs/superpowers/specs/2026-06-03-terminal-v2-routing-design.md`

**Key decisions (locked):**
- Scope: nadfun lifecycle only (token:quote 1:1, token + quote both 18 decimals).
- dexKey: `CURVE`→`nadfun`, `DEX`→`capricorn`, `V2_CURVE`→`nadfun-v2`, `V2_DEX`→`nadswap`.
- feeBps: V1→`100`; `V2_CURVE`→`creator+curve`; `V2_DEX`→`25 + creator + dex` (LP_FEE_RATE=25 from `NadFunPair.sol`). `fee_config` missing → omit `feeBps`.
- swap `pairId`: per-event `market_type` (CURVE→V1_BC, V2_CURVE→V2_BC, DEX/V2_DEX→pool_id).

---

## File Structure

- `src/services/terminal/mod.rs` — add 4 pure helpers + `#[cfg(test)] mod tests`; rewrite `get_pair` and the 3 event converters.
- `src/controllers/terminal/mod.rs` — extend `PairRow` / `SwapEventRow` / `MintEventRow` / `BurnEventRow` + their SQL; add `#[cfg(test)] mod tests` (DB-backed, env-free).
- `docs/terminal-api.md` — rewrite to V2 (external handoff).
- `docs/backend/chang.md` (or repo `chang.md` if backend dir absent) — changelog entry.

No new env vars. No migration.

---

## Task 1: Pure routing helpers (env-free)

**Files:**
- Modify: `src/services/terminal/mod.rs` (add free functions above `pub struct TerminalService`, after `to_truncated_string`)
- Test: `src/services/terminal/mod.rs` (`#[cfg(test)] mod tests` at file end)

- [ ] **Step 1: Write the failing tests**

Append at the end of `src/services/terminal/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const WMON_ADDR: &str = "0x3bd359C1119dA7Da1D913D1C4D2B7c461115433A";
    const LVMON_ADDR: &str = "0xBe3fa50514D9617ce645a02B34F595541AF02b6b";
    const V1_BC: &str = "0x0000000000000000000000000000000000000001";
    const V2_BC: &str = "0x0000000000000000000000000000000000000002";
    const POOL: &str = "0x00000000000000000000000000000000000000aa";
    // Token whose lowercase address is greater than both quotes above.
    const TOKEN_HI: &str = "0xff00000000000000000000000000000000000000";

    #[test]
    fn dex_key_maps_all_market_types() {
        assert_eq!(dex_key_for("CURVE"), "nadfun");
        assert_eq!(dex_key_for("DEX"), "capricorn");
        assert_eq!(dex_key_for("V2_CURVE"), "nadfun-v2");
        assert_eq!(dex_key_for("V2_DEX"), "nadswap");
        assert_eq!(dex_key_for("UNKNOWN"), "nadfun");
    }

    #[test]
    fn pair_id_routes_by_market_type() {
        assert_eq!(pair_id_for("CURVE", None, V1_BC, V2_BC), V1_BC);
        assert_eq!(pair_id_for("V2_CURVE", None, V1_BC, V2_BC), V2_BC);
        assert_eq!(pair_id_for("DEX", Some(POOL), V1_BC, V2_BC), POOL);
        assert_eq!(pair_id_for("V2_DEX", Some(POOL), V1_BC, V2_BC), POOL);
        // DEX with missing pool falls back to the version's bonding curve.
        assert_eq!(pair_id_for("V2_DEX", None, V1_BC, V2_BC), V2_BC);
        assert_eq!(pair_id_for("DEX", None, V1_BC, V2_BC), V1_BC);
    }

    #[test]
    fn fee_bps_v1_is_fixed_100() {
        assert_eq!(fee_bps_for("CURVE", None, None, None), Some(100));
        assert_eq!(fee_bps_for("DEX", Some(500), Some(50), Some(30)), Some(100));
    }

    #[test]
    fn fee_bps_v2_curve_is_creator_plus_curve() {
        assert_eq!(fee_bps_for("V2_CURVE", Some(500), Some(50), Some(30)), Some(550));
    }

    #[test]
    fn fee_bps_v2_dex_is_lp_plus_creator_plus_dex() {
        // 25 (LP) + 500 (creator) + 30 (dex) = 555
        assert_eq!(fee_bps_for("V2_DEX", Some(500), Some(50), Some(30)), Some(555));
    }

    #[test]
    fn fee_bps_v2_missing_fee_config_is_none() {
        assert_eq!(fee_bps_for("V2_CURVE", None, None, None), None);
        assert_eq!(fee_bps_for("V2_DEX", None, None, None), None);
        assert_eq!(fee_bps_for("V2_DEX", Some(500), Some(50), None), None);
    }

    #[test]
    fn order_assets_sorts_by_lowercase_address() {
        // quote (WMON 0x3b...) < token (0xff...) => quote is asset0
        let (a0, a1, quote0) = order_assets(TOKEN_HI, WMON_ADDR);
        assert_eq!(a0, WMON_ADDR);
        assert_eq!(a1, TOKEN_HI);
        assert!(quote0);

        // token (0x00..01) < quote (LVMON 0xbe..) => token is asset0
        let low_token = "0x0000000000000000000000000000000000000abc";
        let (b0, b1, quote0b) = order_assets(low_token, LVMON_ADDR);
        assert_eq!(b0, low_token);
        assert_eq!(b1, LVMON_ADDR);
        assert!(!quote0b);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib services::terminal::tests 2>&1 | tail -20`
Expected: FAIL — `cannot find function dex_key_for` (and the other three).

- [ ] **Step 3: Implement the helpers**

In `src/services/terminal/mod.rs`, immediately after the `to_truncated_string` function (after line 26), insert:

```rust
/// LP fee on the NadSwap (NadFunPair) AMM — fixed 25 BPS (0.25%).
/// Source: nadfun-contract-v2/src/dex/NadFunPair.sol `LP_FEE_RATE = 25`.
const NADSWAP_LP_FEE_BPS: u32 = 25;

/// V1 fixed trading fee, 1% = 100 BPS.
const V1_FEE_BPS: u32 = 100;

/// GeckoTerminal `dexKey` (trading venue identifier) for a market_type.
fn dex_key_for(market_type: &str) -> &'static str {
    match market_type {
        "DEX" => "capricorn",
        "V2_CURVE" => "nadfun-v2",
        "V2_DEX" => "nadswap",
        // "CURVE" and any unknown value fall back to the V1 bonding-curve key.
        _ => "nadfun",
    }
}

/// `pairId` for a market_type. DEX markets use the pool address; curve markets
/// use the version's bonding-curve contract. Falls back to the bonding curve
/// when a DEX market somehow has no pool id.
fn pair_id_for(
    market_type: &str,
    pool_id: Option<&str>,
    v1_bonding_curve: &str,
    v2_bonding_curve: &str,
) -> String {
    match market_type {
        "DEX" => pool_id.unwrap_or(v1_bonding_curve).to_string(),
        "V2_DEX" => pool_id.unwrap_or(v2_bonding_curve).to_string(),
        "V2_CURVE" => v2_bonding_curve.to_string(),
        // "CURVE" and any unknown value map to the V1 bonding curve.
        _ => v1_bonding_curve.to_string(),
    }
}

/// Total trading fee in BPS for a market_type. V1 is fixed at 100. V2 sums the
/// applicable `fee_config` rates; returns `None` when fee_config is missing so
/// callers can omit `feeBps` rather than report a wrong 0%.
fn fee_bps_for(
    market_type: &str,
    creator_fee_rate: Option<i16>,
    curve_protocol_fee_rate: Option<i16>,
    dex_protocol_fee_rate: Option<i16>,
) -> Option<u32> {
    let nonneg = |r: i16| r.max(0) as u32;
    match market_type {
        "V2_CURVE" => Some(nonneg(creator_fee_rate?) + nonneg(curve_protocol_fee_rate?)),
        "V2_DEX" => {
            Some(NADSWAP_LP_FEE_BPS + nonneg(creator_fee_rate?) + nonneg(dex_protocol_fee_rate?))
        }
        // "CURVE", "DEX", and any unknown value use the fixed V1 fee.
        _ => Some(V1_FEE_BPS),
    }
}

/// Order (token, quote) into (asset0, asset1) by ascending lowercase address,
/// matching GeckoTerminal's alphabetical pairing. Returns
/// `(asset0, asset1, is_quote_token0)`.
fn order_assets(token_id: &str, quote_id: &str) -> (String, String, bool) {
    if quote_id.to_lowercase() < token_id.to_lowercase() {
        (quote_id.to_string(), token_id.to_string(), true)
    } else {
        (token_id.to_string(), quote_id.to_string(), false)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib services::terminal::tests 2>&1 | tail -20`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add src/services/terminal/mod.rs
git commit -m "feat(terminal): add quote-aware routing helpers (dexKey, pairId, feeBps, asset order)"
```

---

## Task 2: Controller SQL + rows expose quote_id / market_type / fee_config

**Files:**
- Modify: `src/controllers/terminal/mod.rs` (PairRow + SwapEventRow + MintEventRow + BurnEventRow structs and their queries; `get_pair` and `get_pair_by_pool_id` share `PairRow`, so both queries change)
- Test: `src/controllers/terminal/mod.rs` (`#[cfg(test)] mod tests`)

- [ ] **Step 1: Write the failing DB tests**

Append at the end of `src/controllers/terminal/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const ACCOUNT: &str = "0x000000000000000000000000000000000000Aa01";
    const TOKEN: &str = "0x00000000000000000000000000000000000000F1";
    const POOL: &str = "0x00000000000000000000000000000000000000C9";
    const QUOTE_LVMON: &str = "0xBe3fa50514D9617ce645a02B34F595541AF02b6b";

    fn ctrl(pool: PgPool) -> TerminalController {
        TerminalController::new(Arc::new(crate::db::postgres::PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn seed_token_market(pool: &PgPool, market_type: &str, pool_id: Option<&str>) {
        sqlx::query("INSERT INTO account (account_id,nickname,bio,image_uri) VALUES ($1,'me','','') ON CONFLICT DO NOTHING")
            .bind(ACCOUNT).execute(pool).await.unwrap();
        sqlx::query(r#"INSERT INTO token (token_id,name,symbol,image_uri,creator,description,is_nsfw,is_graduated,is_cto,created_at,transaction_hash,total_supply,version)
            VALUES ($1,'T','T','',$2,NULL,false,false,false,100,'0xtx',1000000000000000000000000000,'V2') ON CONFLICT DO NOTHING"#)
            .bind(TOKEN).bind(ACCOUNT).execute(pool).await.unwrap();
        sqlx::query(r#"INSERT INTO market (market_type,token_id,pool_id,reserve_token,reserve_quote,price,quote_id,latest_trade_at,created_at,volume,ath_price,ath_price_quote)
            VALUES ($1,$2,$3,0,0,1,$4,0,0,0,0,0) ON CONFLICT (token_id) DO NOTHING"#)
            .bind(market_type).bind(TOKEN).bind(pool_id).bind(QUOTE_LVMON).execute(pool).await.unwrap();
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_pair_returns_quote_id_and_fee_config(pool: PgPool) {
        seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
        sqlx::query(r#"INSERT INTO fee_config (pair_id,token_id,creator_fee_rate,curve_protocol_fee_rate,dex_protocol_fee_rate,created_at)
            VALUES ($1,$2,500,50,30,0) ON CONFLICT DO NOTHING"#)
            .bind(POOL).bind(TOKEN).execute(&pool).await.unwrap();

        let row = ctrl(pool).get_pair(TOKEN).await.unwrap();
        assert_eq!(row.quote_id, QUOTE_LVMON);
        assert_eq!(row.market_type, "V2_DEX");
        assert_eq!(row.creator_fee_rate, Some(500));
        assert_eq!(row.dex_protocol_fee_rate, Some(30));
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_pair_without_fee_config_is_none(pool: PgPool) {
        seed_token_market(&pool, "V2_CURVE", None).await;
        let row = ctrl(pool).get_pair(TOKEN).await.unwrap();
        assert_eq!(row.quote_id, QUOTE_LVMON);
        assert_eq!(row.creator_fee_rate, None);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_swap_events_carry_quote_id_and_market_type(pool: PgPool) {
        seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
        sqlx::query(r#"INSERT INTO swap (account_id,token_id,market_type,is_buy,quote_amount,token_amount,reserve_quote,reserve_token,value,created_at,transaction_hash,block_number,tx_index,log_index)
            VALUES ($1,$2,'V2_DEX',true,1000000000000000000,2000000000000000000,100,200,0,123,'0xs',10,0,0) ON CONFLICT DO NOTHING"#)
            .bind(ACCOUNT).bind(TOKEN).execute(&pool).await.unwrap();

        let rows = ctrl(pool).get_events(0, 100).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].quote_id, QUOTE_LVMON);
        assert_eq!(rows[0].market_type, "V2_DEX");
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_mint_events_carry_quote_id(pool: PgPool) {
        seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
        sqlx::query(r#"INSERT INTO mint (token_id,account_id,market_id,quote_amount,token_amount,reserve_quote,reserve_token,created_at,transaction_hash,block_number,tx_index,log_index)
            VALUES ($1,$2,$3,10,20,100,200,123,'0xm',10,0,0) ON CONFLICT DO NOTHING"#)
            .bind(TOKEN).bind(ACCOUNT).bind(POOL).execute(&pool).await.unwrap();

        let rows = ctrl(pool).get_mint_events(0, 100).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].quote_id, QUOTE_LVMON);
    }

    #[sqlx::test(migrations = "./migrations-test")]
    async fn get_burn_events_carry_quote_id(pool: PgPool) {
        seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
        sqlx::query(r#"INSERT INTO burn (token_id,account_id,market_id,quote_amount,token_amount,reserve_quote,reserve_token,created_at,transaction_hash,block_number,tx_index,log_index)
            VALUES ($1,$2,$3,10,20,100,200,123,'0xb',10,0,0) ON CONFLICT DO NOTHING"#)
            .bind(TOKEN).bind(ACCOUNT).bind(POOL).execute(&pool).await.unwrap();

        let rows = ctrl(pool).get_burn_events(0, 100).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].quote_id, QUOTE_LVMON);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib controllers::terminal::tests 2>&1 | tail -30`
Expected: FAIL — compile error: `PairRow`/`SwapEventRow`/`MintEventRow`/`BurnEventRow` have no field `quote_id` (etc.).

- [ ] **Step 3a: Extend `PairRow` and its two queries**

In `src/controllers/terminal/mod.rs`, replace the `PairRow` struct (lines 21-29):

```rust
#[derive(Debug, sqlx::FromRow)]
pub struct PairRow {
    pub token_id: String,
    pub created_at: i64,
    pub transaction_hash: String,
    pub pool_id: Option<String>,
    pub market_type: String,
    pub quote_id: String,
    pub creator: String,
    pub creator_fee_rate: Option<i16>,
    pub curve_protocol_fee_rate: Option<i16>,
    pub dex_protocol_fee_rate: Option<i16>,
}
```

Replace the `get_pair` SQL body (the `SELECT ... FROM token t JOIN market m ... WHERE t.token_id = $1` block) with:

```sql
SELECT
    t.token_id,
    t.created_at,
    t.transaction_hash,
    m.pool_id,
    m.market_type,
    m.quote_id,
    t.creator,
    fc.creator_fee_rate,
    fc.curve_protocol_fee_rate,
    fc.dex_protocol_fee_rate
FROM token t
JOIN market m ON t.token_id = m.token_id
LEFT JOIN fee_config fc ON fc.token_id = t.token_id
WHERE t.token_id = $1
```

Replace the `get_pair_by_pool_id` SQL body the same way (it returns `PairRow`):

```sql
SELECT
    t.token_id,
    t.created_at,
    t.transaction_hash,
    m.pool_id,
    m.market_type,
    m.quote_id,
    t.creator,
    fc.creator_fee_rate,
    fc.curve_protocol_fee_rate,
    fc.dex_protocol_fee_rate
FROM market m
JOIN token t ON m.token_id = t.token_id
LEFT JOIN fee_config fc ON fc.token_id = t.token_id
WHERE m.pool_id = $1
```

- [ ] **Step 3b: Extend `SwapEventRow` and the swap query**

Replace the `SwapEventRow` struct (lines 268-284) by adding two fields after `pub pool_id: Option<String>,`:

```rust
    pub pool_id: Option<String>,
    pub price: BigDecimal,
    pub quote_id: String,
    pub market_type: String,
}
```

In `get_events`, replace the swap SELECT column list / FROM with:

```sql
SELECT
    s.account_id,
    s.token_id,
    s.is_buy,
    s.quote_amount,
    s.token_amount,
    s.reserve_quote,
    s.reserve_token,
    s.created_at,
    s.transaction_hash,
    s.block_number,
    s.tx_index,
    s.log_index,
    m.pool_id,
    m.price,
    m.quote_id,
    s.market_type
FROM swap s
JOIN market m ON s.token_id = m.token_id
WHERE s.block_number >= $1 AND s.block_number <= $2
ORDER BY s.block_number ASC, s.tx_index ASC, s.log_index ASC
```

- [ ] **Step 3c: Extend `MintEventRow` / `BurnEventRow` and their queries**

Add `pub quote_id: String,` as the last field of both `MintEventRow` (after line 300) and `BurnEventRow` (after line 316).

Replace the `get_mint_events` SQL with (alias the table, JOIN market):

```sql
SELECT
    mn.token_id,
    mn.account_id,
    mn.market_id,
    mn.quote_amount,
    mn.token_amount,
    mn.reserve_quote,
    mn.reserve_token,
    mn.created_at,
    mn.transaction_hash,
    mn.block_number,
    mn.tx_index,
    mn.log_index,
    m.quote_id
FROM mint mn
JOIN market m ON m.token_id = mn.token_id
WHERE mn.block_number >= $1 AND mn.block_number <= $2
ORDER BY mn.block_number ASC, mn.tx_index ASC, mn.log_index ASC
```

Replace the `get_burn_events` SQL with:

```sql
SELECT
    bn.token_id,
    bn.account_id,
    bn.market_id,
    bn.quote_amount,
    bn.token_amount,
    bn.reserve_quote,
    bn.reserve_token,
    bn.created_at,
    bn.transaction_hash,
    bn.block_number,
    bn.tx_index,
    bn.log_index,
    m.quote_id
FROM burn bn
JOIN market m ON m.token_id = bn.token_id
WHERE bn.block_number >= $1 AND bn.block_number <= $2
ORDER BY bn.block_number ASC, bn.tx_index ASC, bn.log_index ASC
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib controllers::terminal::tests 2>&1 | tail -30`
Expected: PASS (5 tests). If a seed INSERT fails on a missing column, read the table DDL in `migrations/` and align the column list — do not change assertions.

- [ ] **Step 5: Commit**

```bash
git add src/controllers/terminal/mod.rs
git commit -m "feat(terminal): select quote_id, per-event market_type, and fee_config in queries"
```

---

## Task 3: Quote-aware event converters (pure)

**Files:**
- Modify: `src/services/terminal/mod.rs` (`convert_swap_event`, `convert_mint_event`, `convert_burn_event`)
- Test: `src/services/terminal/mod.rs` (`#[cfg(test)] mod tests` — extend Task 1's module)

- [ ] **Step 1: Write the failing converter tests**

Inside the `mod tests` block added in Task 1, add these tests (and the imports they need at the top of the module):

```rust
    use crate::controllers::terminal::{BurnEventRow, MintEventRow, SwapEventRow};
    use bigdecimal::BigDecimal;

    fn e18(n: u64) -> BigDecimal {
        BigDecimal::from(n) * BigDecimal::from(1_000_000_000_000_000_000u64)
    }

    fn swap_row(market_type: &str, quote_id: &str, is_buy: bool) -> SwapEventRow {
        SwapEventRow {
            account_id: "0xmaker".to_string(),
            token_id: TOKEN_HI.to_string(),
            is_buy,
            quote_amount: e18(1),
            token_amount: e18(2),
            reserve_quote: Some(e18(100)),
            reserve_token: Some(e18(200)),
            created_at: 123,
            transaction_hash: "0xs".to_string(),
            block_number: 10,
            tx_index: Some(0),
            log_index: 0,
            pool_id: Some(POOL.to_string()),
            price: e18(1),
            quote_id: quote_id.to_string(),
            market_type: market_type.to_string(),
        }
    }

    #[test]
    fn swap_v2_dex_lvmon_quote_buy_maps_quote_as_asset0() {
        // TOKEN_HI (0xff..) > LVMON (0xbe..) => quote is asset0.
        let ev = TerminalService::convert_swap_event(swap_row("V2_DEX", LVMON_ADDR, true), V1_BC, V2_BC);
        match ev {
            Event::Swap { asset0_in, asset1_in, asset0_out, asset1_out, price_native, reserves, pair_id, .. } => {
                assert_eq!(asset0_in, Some("1".to_string()));   // quote in
                assert_eq!(asset1_out, Some("2".to_string()));  // token out
                assert_eq!(asset1_in, None);
                assert_eq!(asset0_out, None);
                assert_eq!(reserves.asset0, "100");             // reserve_quote
                assert_eq!(reserves.asset1, "200");             // reserve_token
                assert_eq!(price_native, "2");                  // token/quote
                assert_eq!(pair_id, POOL);                      // V2_DEX -> pool
            }
            _ => panic!("expected swap"),
        }
    }

    #[test]
    fn swap_pair_id_uses_per_event_market_type() {
        // A graduated token's curve-era swap must reference the bonding curve,
        // not the current pool.
        let ev = TerminalService::convert_swap_event(swap_row("V2_CURVE", LVMON_ADDR, true), V1_BC, V2_BC);
        match ev {
            Event::Swap { pair_id, .. } => assert_eq!(pair_id, V2_BC),
            _ => panic!("expected swap"),
        }
    }

    #[test]
    fn swap_v1_wmon_quote_unchanged() {
        // Regression: WMON quote keeps the legacy asset ordering/pricing.
        let ev = TerminalService::convert_swap_event(swap_row("CURVE", WMON_ADDR, true), V1_BC, V2_BC);
        match ev {
            Event::Swap { asset0_in, asset1_out, pair_id, .. } => {
                assert_eq!(asset0_in, Some("1".to_string()));
                assert_eq!(asset1_out, Some("2".to_string()));
                assert_eq!(pair_id, V1_BC);
            }
            _ => panic!("expected swap"),
        }
    }

    #[test]
    fn mint_v2_dex_lvmon_quote_orders_by_quote() {
        let row = MintEventRow {
            token_id: TOKEN_HI.to_string(),
            account_id: "0xmaker".to_string(),
            market_id: POOL.to_string(),
            quote_amount: e18(1),
            token_amount: e18(2),
            reserve_quote: e18(100),
            reserve_token: e18(200),
            created_at: 123,
            transaction_hash: "0xm".to_string(),
            block_number: 10,
            tx_index: 0,
            log_index: 0,
            quote_id: LVMON_ADDR.to_string(),
        };
        match TerminalService::convert_mint_event(row) {
            Event::Join { amount0, amount1, reserves, pair_id, .. } => {
                assert_eq!(amount0, "1");   // quote side
                assert_eq!(amount1, "2");   // token side
                assert_eq!(reserves.asset0, "100");
                assert_eq!(reserves.asset1, "200");
                assert_eq!(pair_id, POOL);  // market_id preserved
            }
            _ => panic!("expected join"),
        }
    }

    #[test]
    fn burn_v2_dex_lvmon_quote_orders_by_quote() {
        let row = BurnEventRow {
            token_id: TOKEN_HI.to_string(),
            account_id: "0xmaker".to_string(),
            market_id: POOL.to_string(),
            quote_amount: e18(1),
            token_amount: e18(2),
            reserve_quote: e18(100),
            reserve_token: e18(200),
            created_at: 123,
            transaction_hash: "0xb".to_string(),
            block_number: 10,
            tx_index: 0,
            log_index: 0,
            quote_id: LVMON_ADDR.to_string(),
        };
        match TerminalService::convert_burn_event(row) {
            Event::Exit { amount0, amount1, pair_id, .. } => {
                assert_eq!(amount0, "1");
                assert_eq!(amount1, "2");
                assert_eq!(pair_id, POOL);
            }
            _ => panic!("expected exit"),
        }
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib services::terminal::tests 2>&1 | tail -30`
Expected: FAIL — `convert_swap_event` takes 1 argument / signature mismatch (and `quote_id` field usage).

- [ ] **Step 3: Rewrite the three converters**

Replace `convert_swap_event` (lines 208-307) with:

```rust
    fn convert_swap_event(
        row: SwapEventRow,
        v1_bonding_curve: &str,
        v2_bonding_curve: &str,
    ) -> Event {
        let decimals_divisor = BigDecimal::from(1_000_000_000_000_000_000u64);

        let quote_amount_decimalized = &row.quote_amount / &decimals_divisor;
        let token_amount_decimalized = &row.token_amount / &decimals_divisor;
        let reserve_quote_decimalized = row
            .reserve_quote
            .as_ref()
            .map(|r| r / &decimals_divisor)
            .unwrap_or_default();
        let reserve_token_decimalized = row
            .reserve_token
            .as_ref()
            .map(|r| r / &decimals_divisor)
            .unwrap_or_default();

        // quote vs token ordering (replaces the old WMON-hardcoded comparison).
        let (_, _, is_quote_token0) = order_assets(&row.token_id, &row.quote_id);

        let (asset0_in, asset1_in, asset0_out, asset1_out) = match (is_quote_token0, row.is_buy) {
            (true, true) => (
                Some(to_truncated_string(&quote_amount_decimalized)),
                None,
                None,
                Some(to_truncated_string(&token_amount_decimalized)),
            ),
            (true, false) => (
                None,
                Some(to_truncated_string(&token_amount_decimalized)),
                Some(to_truncated_string(&quote_amount_decimalized)),
                None,
            ),
            (false, true) => (
                None,
                Some(to_truncated_string(&quote_amount_decimalized)),
                Some(to_truncated_string(&token_amount_decimalized)),
                None,
            ),
            (false, false) => (
                Some(to_truncated_string(&token_amount_decimalized)),
                None,
                None,
                Some(to_truncated_string(&quote_amount_decimalized)),
            ),
        };

        let (reserve_asset0, reserve_asset1) = if is_quote_token0 {
            (
                to_truncated_string(&reserve_quote_decimalized),
                to_truncated_string(&reserve_token_decimalized),
            )
        } else {
            (
                to_truncated_string(&reserve_token_decimalized),
                to_truncated_string(&reserve_quote_decimalized),
            )
        };

        let price_native = if is_quote_token0 {
            to_truncated_string(&(&token_amount_decimalized / &quote_amount_decimalized))
        } else {
            to_truncated_string(&(&quote_amount_decimalized / &token_amount_decimalized))
        };

        Event::Swap {
            block: Block {
                block_number: row.block_number as u64,
                block_timestamp: row.created_at as u64,
            },
            txn_id: row.transaction_hash,
            txn_index: row.tx_index.unwrap_or(0) as u32,
            event_index: row.log_index as u32,
            maker: row.account_id,
            pair_id: pair_id_for(
                &row.market_type,
                row.pool_id.as_deref(),
                v1_bonding_curve,
                v2_bonding_curve,
            ),
            asset0_in,
            asset1_in,
            asset0_out,
            asset1_out,
            price_native,
            reserves: Reserves {
                asset0: reserve_asset0,
                asset1: reserve_asset1,
            },
        }
    }
```

In `convert_mint_event` (lines 309-368) replace the `is_native_token0` line:

```rust
        // Determine token0/token1 by comparing WMON (native) and token_id alphabetically
        let is_native_token0 = WMON.to_lowercase() < row.token_id.to_lowercase();
```

with:

```rust
        let (_, _, is_native_token0) = order_assets(&row.token_id, &row.quote_id);
```

Do the identical replacement in `convert_burn_event` (lines 370-429). (The rest of both functions already branches on `is_native_token0` and needs no change.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --lib services::terminal::tests 2>&1 | tail -30`
Expected: PASS (12 tests total in the module).

- [ ] **Step 5: Commit**

```bash
git add src/services/terminal/mod.rs
git commit -m "feat(terminal): quote-aware event converters with per-event pairId"
```

---

## Task 4: Wire `get_pair` and `get_events` to the helpers

**Files:**
- Modify: `src/services/terminal/mod.rs` (`get_pair`, `get_events` swap mapping, imports)

- [ ] **Step 1: Rewrite `get_pair`**

Replace the asset-ordering + `pair`-building block in `get_pair` (lines 122-152) with:

```rust
        // Order assets by quote_id (replaces the WMON-hardcoded ordering).
        let (asset0_id, asset1_id, _) = order_assets(&pair_row.token_id, &pair_row.quote_id);

        let pair = Pair {
            id: pair_id_for(
                &pair_row.market_type,
                pair_row.pool_id.as_deref(),
                &V1_BONDING_CURVE,
                &V2_BONDING_CURVE,
            ),
            dex_key: dex_key_for(&pair_row.market_type).to_string(),
            asset0_id,
            asset1_id,
            created_at_block_number: Some(block_number),
            created_at_block_timestamp: Some(pair_row.created_at as u64),
            created_at_txn_id: Some(pair_row.transaction_hash),
            creator: Some(pair_row.creator),
            fee_bps: fee_bps_for(
                &pair_row.market_type,
                pair_row.creator_fee_rate,
                pair_row.curve_protocol_fee_rate,
                pair_row.dex_protocol_fee_rate,
            ),
        };

        Ok(PairResponse { pair })
```

- [ ] **Step 2: Pass bonding-curve addresses to the swap converter**

In `get_events`, replace:

```rust
        events.extend(swap_rows.into_iter().map(Self::convert_swap_event));
```

with:

```rust
        events.extend(
            swap_rows
                .into_iter()
                .map(|r| Self::convert_swap_event(r, &V1_BONDING_CURVE, &V2_BONDING_CURVE)),
        );
```

- [ ] **Step 3: Fix imports**

In the `use crate::{ ... }` block at the top of `src/services/terminal/mod.rs`, change the config import line:

```rust
    config::{RPC_URL, V1_BONDING_CURVE, V2_BONDING_CURVE, WMON},
```

to (drop `WMON`, now unused):

```rust
    config::{RPC_URL, V1_BONDING_CURVE, V2_BONDING_CURVE},
```

- [ ] **Step 4: Build + run the full terminal test scope**

Run: `cargo test --lib terminal 2>&1 | tail -30`
Expected: PASS, no warnings about unused `WMON`. If `cargo` reports `WMON` still used, search the file for a leftover reference and convert it to `order_assets`.

- [ ] **Step 5: Run clippy on the touched crate**

Run: `cargo clippy --lib 2>&1 | tail -20`
Expected: no new warnings in `src/services/terminal/` or `src/controllers/terminal/`.

- [ ] **Step 6: Commit**

```bash
git add src/services/terminal/mod.rs
git commit -m "feat(terminal): serve V2_CURVE/V2_DEX in /pair and /events via quote_id"
```

---

## Task 5: External docs + changelog

**Files:**
- Modify: `docs/terminal-api.md` (rewrite to match current code: root paths, 4-way dexKey, V2 feeBps, quote-based pricing, known limits)
- Modify: `docs/backend/chang.md` if it exists, else repo-root `chang.md` (create if neither exists)

- [ ] **Step 1: Rewrite `docs/terminal-api.md`**

Update the Korean doc to reflect, verbatim against the code:
- Endpoints at root: `/latest-block`, `/asset`, `/pair`, `/events`, `/{token_address}` (remove the stale `/terminal/*` prefix).
- `dexKey` table: `CURVE`→`nadfun`, `DEX`→`capricorn`, `V2_CURVE`→`nadfun-v2`, `V2_DEX`→`nadswap` (replace the stale `nadpump`).
- `feeBps`: V1→`100`; `V2_CURVE`→`creator_fee_rate + curve_protocol_fee_rate`; `V2_DEX`→`25 + creator_fee_rate + dex_protocol_fee_rate`; omitted when `fee_config` is missing (replace the stale `30`).
- Note pricing/ordering is by `quote_id` (WMON or LVMON), and tokens/quotes are assumed 18 decimals (nadfun lifecycle scope).
- Known limits: graduation seed liquidity appears only in `dex_mint` (not in `/events` join); `/pair` returns only the current pair.

- [ ] **Step 2: Add a `chang.md` entry**

Append:

```markdown
## Terminal API — V2 라우팅

- `/pair`, `/events`가 `market.quote_id` 기준으로 V2_CURVE / V2_DEX 처리 (WMON 하드코딩 제거).
- dexKey: V2_CURVE=`nadfun-v2`, V2_DEX=`nadswap`. feeBps: V2_CURVE=creator+curve, V2_DEX=25+creator+dex.
- swap pairId를 이벤트별 market_type로 분기. V1 응답 불변.
```

- [ ] **Step 3: Commit**

```bash
git add docs/terminal-api.md docs/backend/chang.md 2>/dev/null || git add docs/terminal-api.md chang.md
git commit -m "docs(terminal): document V2 dexKey, feeBps, and quote-based routing"
```

---

## Task 6: Review + PR

- [ ] **Step 1: Full build + targeted tests with race detector**

Run: `cargo test --lib terminal 2>&1 | tail -20` then `cargo build 2>&1 | tail -5`
Expected: all terminal tests PASS, build OK.

- [ ] **Step 2: Codex review (mandatory pre-PR)**

Run the gstack `/codex review` skill on the diff. Apply AUTO-FIX items immediately; confirm ASK items with the user. Include review commits in the same push.

- [ ] **Step 3: Branch + PR against v2**

```bash
git checkout -b feat/terminal-v2-routing
git push -u origin feat/terminal-v2-routing
gh pr create --base v2 --title "feat(terminal): V2 bonding curve + DEX routing" --body "See docs/superpowers/specs/2026-06-03-terminal-v2-routing-design.md"
```

- [ ] **Step 4: Move changelog entry to `complete.md`**

After merge, move the `chang.md` entry to `complete.md` with date + squash-merge short hash per CLAUDE.md.

---

## Self-Review

**Spec coverage:**
- quote_id replaces WMON in `/pair` + all 3 converters → Tasks 2, 3, 4. ✅
- dexKey 4-way → `dex_key_for` (Task 1), used in `get_pair` (Task 4). ✅
- feeBps V1/V2_CURVE/V2_DEX + None → `fee_bps_for` (Task 1), used in `get_pair` (Task 4). ✅
- swap per-event pairId → `pair_id_for` + `row.market_type` (Tasks 2, 3). ✅
- event source unchanged (swap/mint/burn) → Task 2 keeps tables, only adds quote_id JOIN. ✅
- V1 regression unchanged → `swap_v1_wmon_quote_unchanged` test (Task 3). ✅
- docs → Task 5. ✅

**Type consistency:** `dex_key_for(&str)`, `pair_id_for(&str, Option<&str>, &str, &str)`, `fee_bps_for(&str, Option<i16>, Option<i16>, Option<i16>)`, `order_assets(&str, &str) -> (String, String, bool)`, `convert_swap_event(SwapEventRow, &str, &str)` — used identically in tests and call sites. PairRow/SwapEventRow/MintEventRow/BurnEventRow field names match the seed INSERTs and converter access. ✅

**Placeholder scan:** none. All steps contain concrete code/SQL/commands.

**Open implementation note:** Task 4 assumes `WMON` becomes fully unused in the file; if a later reference remains, Step 4 of Task 4 catches it. `fee_bps_for` BPS composition was verified against `NadFunPair.sol` / `BondingCurve.sol`; if contract economics change, update `NADSWAP_LP_FEE_BPS` and the V2 arms.
