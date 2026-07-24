# Quote-Aware New Event Threshold Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make BUY and SELL entries in `GET /new_event` qualify at `0.0001` of each market's quote token instead of using one hard-coded raw amount.

**Architecture:** Keep the existing controller and category quotas. Extend the BUY and SELL SQL queries through `market.quote_id` to `quote_token.decimals`, bind the exact human threshold as `BigDecimal`, and compare raw swap amounts against `CEIL(0.0001 × 10^decimals)`.

**Tech Stack:** Rust 2024, Tokio, SQLx, PostgreSQL `NUMERIC`, BigDecimal-backed response types.

## Global Constraints

- Use `0.0001` quote tokens as the BUY and SELL threshold.
- Resolve decimals independently for each swap's market quote token.
- Use exact decimal arithmetic; do not introduce floating-point accounting.
- Preserve CREATE 2, BUY 4, and SELL 4 category quotas and final newest-first ordering.
- Preserve the public response shape, raw `amount` representation, and cache behavior.
- Exclude markets that do not have matching quote-token metadata.
- Preserve unrelated and user-authored workspace changes.
- Do not commit, push, open a PR, merge, or deploy without separate authorization.

---

### Task 1: Lock the quote-aware threshold with a database regression test

**Files:**
- Modify: `src/controllers/new_event/mod.rs`
- Test: `src/controllers/new_event/mod.rs`

**Interfaces:**
- Consumes: `NewEventController::fetch_buy_events(limit: i64)` and `NewEventController::fetch_sell_events(limit: i64)`.
- Produces: `new_event_threshold_uses_each_quote_tokens_decimals(pool: PgPool)`, a PostgreSQL-backed regression test covering 18- and 6-decimal quote assets.

- [ ] **Step 1: Add test helpers and the failing integration test**

Append this test module to `src/controllers/new_event/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    fn addr(suffix: &str) -> String {
        format!("0x{:0>40}", suffix)
    }

    fn controller(pool: PgPool) -> NewEventController {
        NewEventController::new(Arc::new(PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    async fn seed_account(pool: &PgPool, account_id: &str) {
        sqlx::query(
            "INSERT INTO account (account_id, nickname, bio, image_uri)
             VALUES ($1, 'account', '', '')
             ON CONFLICT (account_id) DO NOTHING",
        )
        .bind(account_id)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_quote(pool: &PgPool, quote_id: &str, decimals: i32) {
        sqlx::query(
            "INSERT INTO quote_token
                (quote_id, name, symbol, decimals, pyth_feed_id, image_uri)
             VALUES ($1, 'Quote', 'Q', $2, 'feed', '')
             ON CONFLICT (quote_id)
             DO UPDATE SET decimals = EXCLUDED.decimals",
        )
        .bind(quote_id)
        .bind(decimals)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_token_market(
        pool: &PgPool,
        token_id: &str,
        creator: &str,
        quote_id: &str,
    ) {
        sqlx::query(
            "INSERT INTO token
                (token_id, name, symbol, image_uri, creator, created_at,
                 transaction_hash, total_supply)
             VALUES ($1, 'Token', 'T', '', $2, 0, $3, 0)",
        )
        .bind(token_id)
        .bind(creator)
        .bind(format!("token-{token_id}"))
        .execute(pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO market
                (market_type, token_id, price, quote_id, latest_trade_at, created_at)
             VALUES ('DEX', $1, 1, $2, 0, 0)",
        )
        .bind(token_id)
        .bind(quote_id)
        .execute(pool)
        .await
        .unwrap();
    }

    #[allow(clippy::too_many_arguments)]
    async fn seed_swap(
        pool: &PgPool,
        account_id: &str,
        token_id: &str,
        is_buy: bool,
        quote_amount: &str,
        created_at: i64,
        transaction_hash: &str,
    ) {
        sqlx::query(
            "INSERT INTO swap
                (account_id, token_id, market_type, is_buy, quote_amount,
                 token_amount, created_at, transaction_hash, tx_index, log_index)
             VALUES ($1, $2, 'DEX', $3, $4::NUMERIC, 0, $5, $6, 0, 0)",
        )
        .bind(account_id)
        .bind(token_id)
        .bind(is_buy)
        .bind(quote_amount)
        .bind(created_at)
        .bind(transaction_hash)
        .execute(pool)
        .await
        .unwrap();
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn new_event_threshold_uses_each_quote_tokens_decimals(pool: PgPool) {
        let account = addr("acc1");
        let quote_18 = addr("e018");
        let quote_6 = addr("e006");
        let token_18 = addr("a018");
        let token_6 = addr("a006");

        seed_account(&pool, &account).await;
        seed_quote(&pool, &quote_18, 18).await;
        seed_quote(&pool, &quote_6, 6).await;
        seed_token_market(&pool, &token_18, &account, &quote_18).await;
        seed_token_market(&pool, &token_6, &account, &quote_6).await;

        seed_swap(
            &pool,
            &account,
            &token_18,
            true,
            "99999999999999",
            20,
            "buy-below",
        )
        .await;
        seed_swap(
            &pool,
            &account,
            &token_18,
            true,
            "100000000000000",
            10,
            "buy-at",
        )
        .await;
        seed_swap(
            &pool,
            &account,
            &token_6,
            false,
            "99",
            20,
            "sell-below",
        )
        .await;
        seed_swap(
            &pool,
            &account,
            &token_6,
            false,
            "100",
            10,
            "sell-at",
        )
        .await;

        let controller = controller(pool);
        let buys = controller.fetch_buy_events(4).await.unwrap();
        let sells = controller.fetch_sell_events(4).await.unwrap();

        let buy_amounts: Vec<&str> = buys.iter().map(|event| event.amount.as_str()).collect();
        let sell_amounts: Vec<&str> = sells.iter().map(|event| event.amount.as_str()).collect();

        assert_eq!(buy_amounts, vec!["100000000000000"]);
        assert_eq!(sell_amounts, vec!["100"]);
    }
}
```

- [ ] **Step 2: Run the focused test and verify the red state**

Run against a disposable PostgreSQL instance:

```bash
DATABASE_URL=postgresql://localhost:5432/postgres \
  cargo test controllers::new_event::tests::new_event_threshold_uses_each_quote_tokens_decimals
```

Expected: FAIL because the current BUY and SELL predicates require
`quote_amount >= 1000000000000000000`, so both result vectors are empty.

### Task 2: Apply the quote-token decimals in both swap queries

**Files:**
- Modify: `src/controllers/new_event/mod.rs`
- Test: `src/controllers/new_event/mod.rs`

**Interfaces:**
- Consumes: `MIN_NEW_EVENT_QUOTE_AMOUNT`, the `market` and `quote_token` tables, and the regression test from Task 1.
- Produces: BUY and SELL queries that compare raw quote amounts to an exact per-row threshold.

- [ ] **Step 1: Add the shared human-denominated threshold**

Add beside the row types in `src/controllers/new_event/mod.rs`:

```rust
const MIN_NEW_EVENT_QUOTE_AMOUNT: &str = "0.0001";
```

- [ ] **Step 2: Make the BUY query quote-aware**

Add these joins after `JOIN account a2`:

```sql
JOIN market m ON s.token_id = m.token_id
JOIN quote_token qt ON m.quote_id = qt.quote_id
```

Replace the BUY predicate with:

```sql
WHERE s.is_buy = true
  AND s.quote_amount >= CEIL(
      $2 * POWER(10::NUMERIC, qt.decimals)
  )
```

Parse and bind the shared exact decimal after the limit:

```rust
let minimum_quote_amount = MIN_NEW_EVENT_QUOTE_AMOUNT
    .parse::<BigDecimal>()
    .expect("MIN_NEW_EVENT_QUOTE_AMOUNT must be a valid decimal");

// ...
.bind(limit)
.bind(&minimum_quote_amount)
```

- [ ] **Step 3: Make the SELL query quote-aware**

Apply the same `market` and `quote_token` joins, threshold expression, and
second parameter binding to the SELL query, keeping `s.is_buy = false`.

- [ ] **Step 4: Run the focused test and verify the green state**

Run:

```bash
DATABASE_URL=postgresql://localhost:5432/postgres \
  cargo test controllers::new_event::tests::new_event_threshold_uses_each_quote_tokens_decimals
```

Expected: PASS with one qualifying 18-decimal BUY and one qualifying
6-decimal SELL; both immediately-below-boundary swaps remain excluded.

- [ ] **Step 5: Format and repeat the focused test**

Run:

```bash
cargo fmt --all
DATABASE_URL=postgresql://localhost:5432/postgres \
  cargo test controllers::new_event::tests::new_event_threshold_uses_each_quote_tokens_decimals
```

Expected: formatting completes and the focused test remains PASS.

### Task 3: Document, validate, and review the completed change

**Files:**
- Modify: `docs/new-event-api.md`
- Verify: `src/controllers/new_event/mod.rs`
- Verify: `docs/superpowers/specs/2026-07-24-new-event-quote-threshold-design.md`
- Verify: `docs/superpowers/plans/2026-07-24-new-event-quote-threshold.md`

**Interfaces:**
- Consumes: the completed quote-aware query behavior.
- Produces: accurate public documentation and a validated local diff.

- [ ] **Step 1: Document the threshold semantics**

Add the following to `docs/new-event-api.md` after the event-type table:

```markdown
### BUY/SELL 노출 기준

- 각 market의 `quote_token.decimals`를 기준으로 `0.0001 quote token` 이상인
  BUY/SELL 이벤트만 노출됩니다.
- 예: WETH(18 decimals)는 raw `100000000000000` 이상, 6-decimal quote
  token은 raw `100` 이상입니다.
- 응답의 `amount`는 기존과 같이 raw quote amount 문자열입니다.
```

- [ ] **Step 2: Run repository validation**

Run:

```bash
cargo fmt --all -- --check
DATABASE_URL=postgresql://localhost:5432/postgres \
  cargo test controllers::new_event::tests::new_event_threshold_uses_each_quote_tokens_decimals
cargo test --lib
SQLX_OFFLINE=true cargo build --release
git diff --check
```

Expected:

- formatting passes;
- the focused database test passes;
- DB-independent library tests pass;
- database-backed tests outside the focused disposable-PostgreSQL run may
  require their own integration services and must be reported if skipped;
- the SQLx offline release build passes; and
- the diff has no whitespace errors.

- [ ] **Step 3: Review the final diff**

Inspect:

```bash
git status --short
git diff -- src/controllers/new_event/mod.rs docs/new-event-api.md \
  docs/superpowers/specs/2026-07-24-new-event-quote-threshold-design.md \
  docs/superpowers/plans/2026-07-24-new-event-quote-threshold.md
```

Confirm:

- only the approved threshold behavior and its test/docs changed;
- both BUY and SELL use the same exact parameter and decimals expression;
- category limits, ordering, response serialization, and cache code are unchanged;
- no unrelated user-authored file is modified by this task.

- [ ] **Step 4: Request a correctness review**

Have a code reviewer inspect the completed diff for query correctness,
decimal-boundary behavior, regressions, maintainability, and test gaps. Resolve
all merge-blocking findings and repeat the focused test after any code change.

No commit, push, pull request, merge, or deployment is part of this plan.
