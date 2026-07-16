# Terminal API DEX-Only Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Terminal `/pair` and `/events` expose only `DEX` and `V2_DEX` market data while leaving metadata, asset, latest-block, response shapes, and ordering unchanged.

**Architecture:** Add explicit positive allowlists to the four existing SQL queries in `TerminalController`; do not add service-side filtering or alter conversion logic. Keep the current joins so `/pair`, mint, and burn filter by the joined current `market.market_type`, while swap filters by the historical event's own `swap.market_type`.

**Tech Stack:** Rust 2024, Axum 0.7, sqlx 0.8/PostgreSQL, `#[sqlx::test]`, Markdown API documentation.

## Global Constraints

- The integration base and PR base are `v2`, not `main`; this feature branch is `feat/terminal-dex-only`, forked from `v2`.
- Do not create a PR directly from an unrelated working branch; use a fresh `feat/*` or `fix/*` branch.
- All EVM addresses, including request values and new test fixtures, must use canonical EIP-55 checksum form; never add `LOWER()` address comparisons.
- `migrations/` is a submodule. This change has no schema migration: do not modify the submodule gitlink or `migrations-test/` symlinks.
- If a PR is eventually merged, use squash without `--delete-branch`, then run `git fetch origin v2:v2` and confirm `git log --oneline -1 v2` matches `origin/v2`.
- Preserve `/asset`, `/{token_address}`, `/latest-block`, address normalization, response JSON, fee/amount calculations, block validation, three-query parallelism, and final `(block_number, tx_index, log_index)` ascending sort.
- Run SQLx tests only against an explicit disposable non-production PostgreSQL URL: `DATABASE_URL="$DATABASE_TEST_URL"`.

---

## File Structure

- `src/controllers/terminal/mod.rs` — add DEX allowlists to `get_pair`, `get_events`, `get_mint_events`, and `get_burn_events`; update their DB-backed regression tests.
- `docs/terminal-api.md` — describe the externally visible DEX-only policy and the unchanged endpoints.

No service, router, type, schema, migration, or configuration file changes are needed.

---

### Task 1: Restrict `/pair` to DEX and V2_DEX

**Files:**
- Modify: `src/controllers/terminal/mod.rs:89-118`
- Test: `src/controllers/terminal/mod.rs:350-399`

**Interfaces:**
- Consumes: `TerminalController::get_pair(&self, token_id: &str) -> anyhow::Result<PairRow>` and the existing `market m` join.
- Produces: the same method and row type, returning a row only when `m.market_type` is exactly `DEX` or `V2_DEX`; an excluded market continues through `TerminalService::get_pair`'s existing `AppError::NotFound` mapping.

- [ ] **Step 1: Make the existing token fixture EIP-55 canonical and add the failing allowlist test**

In `src/controllers/terminal/mod.rs`, change only the `TOKEN` constant's checksum casing:

```rust
const TOKEN: &str = "0x00000000000000000000000000000000000000f1";
```

Keep `get_pair_without_fee_config_is_none`, but seed an allowed market:

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn get_pair_without_fee_config_is_none(pool: PgPool) {
    seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
    let row = ctrl(pool).get_pair(TOKEN).await.unwrap();
    assert_eq!(row.quote_id, QUOTE_LVMON);
    assert_eq!(row.creator_fee_rate, None);
}
```

Immediately after it, add:

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn get_pair_only_returns_dex_markets(pool: PgPool) {
    seed_token_market(&pool, "V2_DEX", Some(POOL)).await;

    for market_type in ["DEX", "V2_DEX"] {
        sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
            .bind(market_type)
            .bind(TOKEN)
            .execute(&pool)
            .await
            .unwrap();

        let row = ctrl(pool.clone()).get_pair(TOKEN).await.unwrap();
        assert_eq!(row.market_type, market_type);
    }

    for market_type in ["CURVE", "V2_CURVE"] {
        sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
            .bind(market_type)
            .bind(TOKEN)
            .execute(&pool)
            .await
            .unwrap();

        assert!(
            ctrl(pool.clone()).get_pair(TOKEN).await.is_err(),
            "{market_type} pair must be filtered out"
        );
    }
}
```

- [ ] **Step 2: Run the pair test and verify RED**

Initialize the migration submodule once if this worktree has not done so:

```bash
git submodule update --init migrations
test -n "$DATABASE_TEST_URL"
```

Run:

```bash
DATABASE_URL="$DATABASE_TEST_URL" cargo test --lib controllers::terminal::tests::get_pair_only_returns_dex_markets -- --nocapture
```

Expected: FAIL on the first excluded market with `CURVE pair must be filtered out`, because the current query accepts every `market_type`.

- [ ] **Step 3: Add the minimal `/pair` SQL allowlist**

In `TerminalController::get_pair`, replace the final predicate with:

```sql
WHERE t.token_id = $1
  AND m.market_type IN ('DEX', 'V2_DEX')
```

Do not change `get_pair_by_pool_id`; it is not used by public `/pair` and is explicitly outside the approved scope.

- [ ] **Step 4: Run pair regressions and verify GREEN**

Run:

```bash
DATABASE_URL="$DATABASE_TEST_URL" cargo test --lib controllers::terminal::tests::get_pair_ -- --nocapture
```

Expected: PASS for `get_pair_returns_quote_id_and_fee_config`, `get_pair_without_fee_config_is_none`, and `get_pair_only_returns_dex_markets`. The returned `PairRow` fields for `DEX`/`V2_DEX` remain unchanged.

- [ ] **Step 5: Commit the pair slice**

```bash
git add src/controllers/terminal/mod.rs
git commit -m "fix(terminal): restrict pair lookup to DEX markets"
```

---

### Task 2: Restrict swap, join, and exit event queries

**Files:**
- Modify: `src/controllers/terminal/mod.rs:176-293`
- Test: `src/controllers/terminal/mod.rs:400-461`

**Interfaces:**
- Consumes: `get_events(u64, u64) -> Result<Vec<SwapEventRow>>`, `get_mint_events(u64, u64) -> Result<Vec<MintEventRow>>`, and `get_burn_events(u64, u64) -> Result<Vec<BurnEventRow>>`.
- Produces: unchanged row types and method signatures; swap eligibility comes from `s.market_type`, while mint/burn eligibility comes from joined `m.market_type`. Each query retains its inclusive block range and ascending SQL ordering.

- [ ] **Step 1: Replace the old V2_CURVE-only swap regression with a failing positive-allowlist test**

Replace `get_events_excludes_v2_curve` with:

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn get_events_only_returns_dex_and_uses_swap_market_type(pool: PgPool) {
    // Current market stays V2_DEX; historical eligibility must use each swap row.
    seed_token_market(&pool, "V2_DEX", Some(POOL)).await;

    for (market_type, tx_hash, block_number) in [
        ("CURVE", "0xcurve", 10_i64),
        ("DEX", "0xdex", 11_i64),
        ("V2_CURVE", "0xv2curve", 12_i64),
        ("V2_DEX", "0xv2dex", 13_i64),
    ] {
        sqlx::query(
            r#"INSERT INTO swap
                (account_id,token_id,market_type,is_buy,quote_amount,token_amount,
                 reserve_quote,reserve_token,value,created_at,transaction_hash,
                 block_number,tx_index,log_index)
               VALUES ($1,$2,$3,true,1,2,100,200,0,$4,$5,$4,0,0)"#,
        )
        .bind(ACCOUNT)
        .bind(TOKEN)
        .bind(market_type)
        .bind(block_number)
        .bind(tx_hash)
        .execute(&pool)
        .await
        .unwrap();
    }

    let rows = ctrl(pool).get_events(0, 100).await.unwrap();
    let markets: Vec<&str> = rows.iter().map(|row| row.market_type.as_str()).collect();
    let transactions: Vec<&str> = rows
        .iter()
        .map(|row| row.transaction_hash.as_str())
        .collect();

    assert_eq!(markets, vec!["DEX", "V2_DEX"]);
    assert_eq!(transactions, vec!["0xdex", "0xv2dex"]);
}
```

This one test proves both allowed values, both Curve exclusions, historical `swap.market_type` selection despite a current `V2_DEX` market, and retained block ordering.

- [ ] **Step 2: Upgrade mint and burn tests to fail on Curve and future market types**

The current schema constrains `market.market_type` to known values. To prove that the SQL positive allowlist remains closed when a future migration permits another value, add this test-only helper immediately after `seed_token_market` in the existing test module. It removes only the temporary SQLx database's `market_type` CHECK constraint; production migrations remain untouched:

```rust
async fn allow_future_market_type(pool: &PgPool) {
    sqlx::query(
        r#"
            DO $$
            DECLARE
                constraint_name text;
            BEGIN
                FOR constraint_name IN
                    SELECT c.conname
                    FROM pg_constraint c
                    WHERE c.conrelid = 'market'::regclass
                      AND c.contype = 'c'
                      AND pg_get_constraintdef(c.oid) LIKE '%market_type%'
                LOOP
                    EXECUTE format(
                        'ALTER TABLE market DROP CONSTRAINT %I',
                        constraint_name
                    );
                END LOOP;
            END
            $$
        "#,
    )
    .execute(pool)
    .await
    .unwrap();
}
```

Replace `get_mint_events_carry_quote_id` with:

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn get_mint_events_only_return_dex_markets(pool: PgPool) {
    seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
    sqlx::query(r#"INSERT INTO mint (token_id,account_id,market_id,quote_amount,token_amount,reserve_quote,reserve_token,created_at,transaction_hash,block_number,tx_index,log_index)
        VALUES ($1,$2,$3,10,20,100,200,123,'0xm',10,0,0)"#)
        .bind(TOKEN).bind(ACCOUNT).bind(POOL).execute(&pool).await.unwrap();

    allow_future_market_type(&pool).await;

    for market_type in ["DEX", "V2_DEX"] {
        sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
            .bind(market_type).bind(TOKEN).execute(&pool).await.unwrap();
        let rows = ctrl(pool.clone()).get_mint_events(0, 100).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].quote_id, QUOTE_LVMON);
    }

    for market_type in ["FUTURE_MARKET", "CURVE", "V2_CURVE"] {
        sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
            .bind(market_type).bind(TOKEN).execute(&pool).await.unwrap();
        let rows = ctrl(pool.clone()).get_mint_events(0, 100).await.unwrap();
        assert!(rows.is_empty(), "{market_type} mint must be filtered out");
    }
}
```

Replace `get_burn_events_carry_quote_id` with:

```rust
#[sqlx::test(migrations = "./migrations-test")]
async fn get_burn_events_only_return_dex_markets(pool: PgPool) {
    seed_token_market(&pool, "V2_DEX", Some(POOL)).await;
    sqlx::query(r#"INSERT INTO burn (token_id,account_id,market_id,quote_amount,token_amount,reserve_quote,reserve_token,created_at,transaction_hash,block_number,tx_index,log_index)
        VALUES ($1,$2,$3,10,20,100,200,123,'0xb',10,0,0)"#)
        .bind(TOKEN).bind(ACCOUNT).bind(POOL).execute(&pool).await.unwrap();

    allow_future_market_type(&pool).await;

    for market_type in ["DEX", "V2_DEX"] {
        sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
            .bind(market_type).bind(TOKEN).execute(&pool).await.unwrap();
        let rows = ctrl(pool.clone()).get_burn_events(0, 100).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].quote_id, QUOTE_LVMON);
    }

    for market_type in ["FUTURE_MARKET", "CURVE", "V2_CURVE"] {
        sqlx::query("UPDATE market SET market_type = $1 WHERE token_id = $2")
            .bind(market_type).bind(TOKEN).execute(&pool).await.unwrap();
        let rows = ctrl(pool.clone()).get_burn_events(0, 100).await.unwrap();
        assert!(rows.is_empty(), "{market_type} burn must be filtered out");
    }
}
```

- [ ] **Step 3: Run the event tests and verify RED**

Run:

```bash
DATABASE_URL="$DATABASE_TEST_URL" cargo test --lib controllers::terminal::tests::get_events_only_returns_dex_and_uses_swap_market_type -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test --lib controllers::terminal::tests::get_mint_events_only_return_dex_markets -- --nocapture
DATABASE_URL="$DATABASE_TEST_URL" cargo test --lib controllers::terminal::tests::get_burn_events_only_return_dex_markets -- --nocapture
```

Expected: swap FAILS with a left-hand list that still contains `CURVE`; mint FAILS with `FUTURE_MARKET mint must be filtered out`; burn FAILS with `FUTURE_MARKET burn must be filtered out`. The latter two failures demonstrate that the pre-change queries expose a newly permitted market value because they have no market predicate.

- [ ] **Step 4: Add the three minimal SQL allowlists**

In the swap query, replace the two comments and negative condition with:

```sql
WHERE s.block_number >= $1 AND s.block_number <= $2
  AND s.market_type IN ('DEX', 'V2_DEX')
ORDER BY s.block_number ASC, s.tx_index ASC, s.log_index ASC
```

In the mint query, use:

```sql
WHERE mn.block_number >= $1 AND mn.block_number <= $2
  AND m.market_type IN ('DEX', 'V2_DEX')
ORDER BY mn.block_number ASC, mn.tx_index ASC, mn.log_index ASC
```

In the burn query, use:

```sql
WHERE bn.block_number >= $1 AND bn.block_number <= $2
  AND m.market_type IN ('DEX', 'V2_DEX')
ORDER BY bn.block_number ASC, bn.tx_index ASC, bn.log_index ASC
```

The positive `IN` allowlist deliberately excludes any future or unknown market type without another code change. Do not filter swap through `m.market_type`.

- [ ] **Step 5: Run terminal controller tests and verify GREEN**

Run:

```bash
DATABASE_URL="$DATABASE_TEST_URL" cargo test --lib controllers::terminal::tests -- --nocapture
```

Expected: all Terminal controller SQLx tests PASS, including `FUTURE_MARKET`, `CURVE`, and `V2_CURVE` exclusion for both mint and burn. The new transaction assertion stays in ascending block order, and inspection confirms the untouched service still runs the three queries with `tokio::join!` and sorts the combined events by `(block_number, tx_index, log_index)`.

- [ ] **Step 6: Commit the events slice**

```bash
git add src/controllers/terminal/mod.rs
git commit -m "fix(terminal): restrict event queries to DEX markets"
```

---

### Task 3: Document policy and run final verification

**Files:**
- Modify: `docs/terminal-api.md:3-64,147-203,275-306,356-360`

**Interfaces:**
- Consumes: the implemented SQL behavior from Tasks 1-2.
- Produces: external documentation that distinguishes DEX-only market endpoints from unchanged asset, metadata, and block endpoints.

- [ ] **Step 1: Add the exact exposure policy near the overview**

After the root-path note at `docs/terminal-api.md:13`, add:

```markdown
> **시장 노출 정책**: 시장 데이터 엔드포인트인 `/pair`와 `/events`는 `DEX`, `V2_DEX`만 제공합니다. `CURVE`, `V2_CURVE` 및 알 수 없는 시장 유형은 노출하지 않습니다. `/asset`, `/{token_address}`, `/latest-block`의 동작은 시장 유형과 무관하며 변경되지 않습니다.
```

- [ ] **Step 2: Update pair and event semantics without changing schemas**

Replace the opening `/pair` paragraph with:

```markdown
거래쌍(Pair) 정보를 조회합니다. 쿼리 `id`는 **토큰 주소**이며, 토큰의 현재 `market_type`이 `DEX` 또는 `V2_DEX`인 경우에만 현재 페어를 반환합니다. `CURVE`, `V2_CURVE` 또는 알 수 없는 시장 유형은 `404`로 처리됩니다.
```

Replace the opening `/events` paragraph with:

```markdown
특정 블록 범위 내의 DEX 거래 이벤트(swap / join / exit)를 조회합니다. swap은 이벤트 자신의 `market_type`, join/exit는 연결된 현재 market을 기준으로 `DEX`, `V2_DEX`만 반환하며 Curve 및 알 수 없는 시장 유형은 제외합니다.
```

Replace the swap `pairId` field description with:

```markdown
| `pairId` | string | DEX/V2_DEX 이벤트의 풀 주소. 필터 기준은 이벤트 자신의 `market_type` |
```

Replace known limitation 2 with these two items:

```markdown
2. **`/pair`는 DEX 현재 페어만 반환**: 현재 시장이 `DEX` 또는 `V2_DEX`인 토큰의 현재 페어 하나만 반환하며 Curve 토큰은 `404`입니다.
3. **Curve 이벤트 미노출**: `/events`는 `DEX`, `V2_DEX` swap/join/exit만 반환하므로 졸업 전 Curve 거래 이력은 포함하지 않습니다.
```

Keep the JSON examples and TypeScript interfaces unchanged.

- [ ] **Step 3: Run formatting, targeted tests, lint, and scope guards**

Run:

```bash
cargo fmt --check
DATABASE_URL="$DATABASE_TEST_URL" cargo test --lib controllers::terminal::tests -- --nocapture
cargo test --lib services::terminal::tests -- --nocapture
cargo clippy --all-targets --all-features -- -D warnings
git diff --check
git diff --exit-code v2...HEAD -- migrations migrations-test src/services/terminal/mod.rs src/router/terminal src/types/terminal
```

Expected: formatting, targeted tests, service conversion regressions, Clippy, and whitespace checks PASS; the final scope guard prints no diff and exits 0. If the repository has unrelated baseline Clippy warnings, record them separately and still require zero new warning in the changed controller.

- [ ] **Step 4: Review the complete diff against the approved spec**

Run:

```bash
git diff v2...HEAD -- src/controllers/terminal/mod.rs docs/terminal-api.md
git status --short
```

Expected: production changes are exactly four SQL `IN ('DEX', 'V2_DEX')` predicates; tests cover DEX/V2_DEX inclusion and Curve/V2Curve exclusion; docs describe the same behavior; no migration, service, schema, route, response, or unrelated file is changed.

- [ ] **Step 5: Commit documentation**

```bash
git add docs/terminal-api.md
git commit -m "docs(terminal): document DEX-only market exposure"
```

---

## Test-Environment Caveat

The isolated planning worktree currently has an uninitialized `migrations` submodule (`git submodule status` shows a leading `-`), so its `migrations-test/*.sql` symlinks are dangling and SQLx tests cannot run there yet. The implementer must run `git submodule update --init migrations` with repository access, provide a disposable `DATABASE_TEST_URL`, and never point the SQLx harness at production. This setup should not produce a gitlink diff.

## Completion Checklist

- [ ] `/pair` returns rows only for current `DEX`/`V2_DEX`; Curve follows the existing not-found path.
- [ ] swap filters on `s.market_type`; mint and burn filter on joined `m.market_type`.
- [ ] All four predicates are positive allowlists; mint and burn regression tests explicitly prove a simulated `FUTURE_MARKET` remains hidden.
- [ ] DEX/V2_DEX row fields, JSON shape, parallel querying, inclusive ranges, and event ordering remain unchanged.
- [ ] `/asset`, `/{token_address}`, `/latest-block`, and `get_pair_by_pool_id` remain unchanged.
- [ ] `docs/terminal-api.md` matches the shipped policy.
- [ ] No migration submodule or symlink changes exist.
