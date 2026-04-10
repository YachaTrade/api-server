# Price Quote-Aware Refactor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the `latest_price` scalar CTE (single global price) with a per-row `LEFT JOIN LATERAL` that looks up the USD price for each market's `quote_id`, so V2 tokens with non-WMON quote assets get the correct USD conversion.

**Architecture:** observer's V2 `price` table is now keyed by `(quote_id, block_number)` with an index `idx_price_quote_block (quote_id, block_number DESC)`. A single `SELECT price FROM price ORDER BY created_at DESC LIMIT 1` returns an arbitrary quote's most-recent price (whichever was inserted last), which is incorrect for any token whose quote is not the most-recently-updated one. The fix is a per-market LATERAL subquery scoped by `m.quote_id`.

**Tech Stack:** Rust / sqlx (runtime SQL), PostgreSQL, Axum.

---

## Background

The `latest_price` CTE pattern exists in 10 files, ~19 queries total:

```sql
WITH latest_price AS (
    SELECT price FROM price ORDER BY created_at DESC LIMIT 1
)
...
CROSS JOIN latest_price lp
```

After observer's V2 migration:
- `price` table has `quote_id VARCHAR(42) NOT NULL DEFAULT '<WMON>'`
- PK is `(quote_id, block_number)` — composite
- Index `idx_price_quote_block (quote_id, block_number DESC)` exists
- `idx_price_created_at` also exists but we won't use it

For each query, `m.quote_id` (from the market table) tells us which quote asset this token trades against. We must look up the USD price for THAT quote, not a global scalar.

**Semantic implication:** The API response field `native_price` currently means "MON/USD". After this refactor it means "this token's quote-asset / USD". For V1 tokens (quote = WMON = MON) the value is unchanged. For V2 non-WMON tokens the value becomes correct instead of nonsense. Rename is tech debt deferred; frontend comms out-of-scope for this plan.

## Scope / File Inventory

**19 queries across 10 files** — two categories:

### Category A — market already in FROM / JOIN clause (direct swap)

| File | # queries | Notes |
|------|-----------|-------|
| `src/controllers/token/order.rs` | 3 | CreationTime, LatestTrade, MarketCap branches (MarketCap uses derived table aliased `m`) |
| `src/controllers/token/metadata.rs` | 1 | |
| `src/controllers/token/create.rs` | 1 | Recently refactored — `paged_tokens` CTE + outer `JOIN market m` |
| `src/controllers/trading/market.rs` | 1 | `FROM market m` |
| `src/controllers/trading/position.rs` | 2 | hold_token_by_account + other |
| `src/controllers/trend/mod.rs` | 1 | |
| `src/controllers/search/mod.rs` | 3 | 3 branches (by-name, by-symbol, by-address) |
| `src/controllers/hype/mod.rs` | 3 | 3 branches (uses alias `h` for hype table) |

**Total: 15 queries across 8 files.**

### Category B — `FROM swap s` without market join (must add `JOIN market m`)

| File | # queries | Notes |
|------|-----------|-------|
| `src/controllers/trading/swap_history.rs` | 2 | `get_swaps_by_account` (has `recent_swaps` CTE), `get_swaps_by_token` (single query) |
| `src/controllers/chester/mod.rs` | 1 | `get_swap_history` (has `recent_swaps` CTE) |

**Total: 3 queries across 2 files.**

**Note:** `trading/market.rs` was listed in both categories during discovery — it's actually Category A (has `FROM market m JOIN token t`).

## The Transformation Pattern

### Category A: remove CTE, swap CROSS JOIN for LATERAL

**Before:**
```sql
WITH latest_price AS (
    SELECT price FROM price ORDER BY created_at DESC LIMIT 1
)
SELECT
    ...
    (m.price * COALESCE(lp.price, 0)) as token_price,
    COALESCE(lp.price, 0) as native_price,
    (m.price * COALESCE(lp.price, 0)) as price_usd,
    ...
FROM ...
JOIN market m ON ...
CROSS JOIN latest_price lp
WHERE ...
```

**After:**
```sql
SELECT
    ...
    (m.price * COALESCE(lp.price, 0)) as token_price,
    COALESCE(lp.price, 0) as native_price,
    (m.price * COALESCE(lp.price, 0)) as price_usd,
    ...
FROM ...
JOIN market m ON ...
LEFT JOIN LATERAL (
    SELECT p.price
    FROM price p
    WHERE p.quote_id = m.quote_id
    ORDER BY p.block_number DESC
    LIMIT 1
) lp ON true
WHERE ...
```

**Rules:**
1. Delete the `WITH latest_price AS (...)` CTE block entirely. If other CTEs remain (e.g. `paged_tokens`, `claimed_totals`), preserve them and the `WITH` keyword with comma chaining adjusted.
2. Replace `CROSS JOIN latest_price lp` with the `LEFT JOIN LATERAL (...) lp ON true` block, positioned **immediately after** the `JOIN market m ON ...` line (required so `m.quote_id` is in scope).
3. Do not touch the SELECT expressions — `COALESCE(lp.price, 0)` stays identical. Semantics change implicitly.
4. If the query has a derived table aliased as `m` (e.g. `token/order.rs` MarketCap branch), ensure the derived table SELECT projects `m.quote_id`. (Already done in commit `af7a96d` — verify during edit.)

### Category B: add `JOIN market m`, then apply Category A pattern

**Before (chester example):**
```sql
WITH latest_price AS (
    SELECT price FROM price ORDER BY created_at DESC LIMIT 1
),
recent_swaps AS (
    SELECT s.token_id, s.is_buy, s.quote_amount, ...
    FROM swap s
    ...
)
SELECT
    ...
    COALESCE(lp.price, 0) as native_price,
    ...
FROM recent_swaps rs
JOIN token t ON rs.token_id = t.token_id
JOIN account a ON t.creator = a.account_id
LEFT JOIN account_x ax ON a.account_id = ax.account_id
CROSS JOIN latest_price lp
ORDER BY rs.created_at DESC
```

**After:**
```sql
WITH recent_swaps AS (
    SELECT s.token_id, s.is_buy, s.quote_amount, ...
    FROM swap s
    ...
)
SELECT
    ...
    COALESCE(lp.price, 0) as native_price,
    ...
FROM recent_swaps rs
JOIN token t ON rs.token_id = t.token_id
JOIN account a ON t.creator = a.account_id
LEFT JOIN account_x ax ON a.account_id = ax.account_id
JOIN market m ON rs.token_id = m.token_id
LEFT JOIN LATERAL (
    SELECT p.price
    FROM price p
    WHERE p.quote_id = m.quote_id
    ORDER BY p.block_number DESC
    LIMIT 1
) lp ON true
ORDER BY rs.created_at DESC
```

**Rules:**
1. Same as Category A plus:
2. Add `JOIN market m ON <swap_table_alias>.token_id = m.token_id` immediately before the LATERAL join. Use the correct alias: `rs` in the `get_swaps_by_account` and chester CTE-based queries; `s` in the `get_swaps_by_token` direct query.
3. Do NOT add `m.*` to the SELECT — this join is only for `m.quote_id` scoping. The SELECT stays unchanged.

## Testing Strategy

SQL changes are not meaningfully TDDable inside this repo (no integration test harness for these controllers, sqlx macros validate at runtime not compile). Verification steps:

1. **Per task:** `cargo check` compiles cleanly (catches Rust type errors only).
2. **End of phase 1 and phase 2:** `cargo check` + `cargo fmt` clean.
3. **Before PR:** On a dev DB that has V2 schema applied AND price table populated with at least two distinct quote_id values:
   - Pick 2 representative queries (one Category A, one Category B) and run `EXPLAIN (ANALYZE, BUFFERS)` to confirm `idx_price_quote_block` is used.
   - For one V1 token (quote = WMON), call the corresponding endpoint before/after and diff response: should be byte-identical.
4. **Smoke test** after merge, before prod: hit each of the 10 endpoints with a real account id and confirm 200 response + non-zero `native_price` field for V1 tokens.

---

## Task 0: Create feature branch

**Files:** (none — git state)

- [ ] **Step 1: Pull latest v2 and branch**

```bash
cd /Users/gyu/project/nads-pump/api-server
git checkout v2
git pull
git checkout -b feat/price_quote_aware
```

Expected: `Switched to a new branch 'feat/price_quote_aware'`

- [ ] **Step 2: Verify clean baseline**

```bash
cargo check 2>&1 | tail -5
```

Expected: Only pre-existing `unused import: env` warning in `src/cors.rs:5`. No errors.

---

## Task 1: token/metadata.rs — simplest Category A (1 query)

**Files:**
- Modify: `src/controllers/token/metadata.rs:83-125` (approximate — the `WITH latest_price AS` CTE through the `CROSS JOIN latest_price lp` line)

Start with this file because it has exactly 1 query and the cleanest structure — proves the pattern before multiplying.

- [ ] **Step 1: Read the file to confirm current state**

```bash
# Read the fetch_token_metadata function
```

Confirm the query has:
```sql
WITH latest_price AS (
    SELECT price
    FROM price
    ORDER BY created_at DESC
    LIMIT 1
)
SELECT
    ...
FROM token t
JOIN account a ON t.creator = a.account_id
LEFT JOIN account_x ax ON a.account_id = ax.account_id
JOIN market m ON t.token_id = m.token_id
CROSS JOIN latest_price lp
WHERE t.token_id = $1
```

- [ ] **Step 2: Delete the `WITH latest_price AS (...)` CTE header**

Remove these lines:
```sql
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                )
```

And make sure the `SELECT` that followed the closing `)` now starts the query directly.

- [ ] **Step 3: Replace `CROSS JOIN latest_price lp` with LATERAL**

Replace:
```sql
                JOIN market m ON t.token_id = m.token_id
                CROSS JOIN latest_price lp
                WHERE t.token_id = $1
```

With:
```sql
                JOIN market m ON t.token_id = m.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp ON true
                WHERE t.token_id = $1
```

- [ ] **Step 4: cargo check**

```bash
cargo check 2>&1 | tail -10
```

Expected: clean (only pre-existing warning).

- [ ] **Step 5: Commit**

```bash
git add src/controllers/token/metadata.rs
git commit -m "refactor(metadata): replace latest_price CTE with per-quote LATERAL lookup"
```

---

## Task 2: token/create.rs — Category A with paged_tokens CTE (1 query)

**Files:**
- Modify: `src/controllers/token/create.rs:152-216` (latest_price CTE and CROSS JOIN line, inside the `WITH ... paged_tokens ... claimed_totals ...` CTE chain)

This file has multiple CTEs — we must delete only the `latest_price` CTE and keep `paged_tokens` and `claimed_totals`.

- [ ] **Step 1: Read current structure**

The query starts:
```sql
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                ),
                paged_tokens AS (
                    SELECT t.*
                    FROM token t
                    WHERE t.creator = $1
                    ORDER BY t.created_at DESC
                    LIMIT $2 OFFSET $3
                ),
                claimed_totals AS (
                    SELECT token_id, SUM(amount) as claimed_amount
                    FROM creator_treasury_claim_history
                    WHERE account_id = $1
                    GROUP BY token_id
                )
                SELECT
```

- [ ] **Step 2: Delete the latest_price CTE and its trailing comma**

Replace:
```sql
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                ),
                paged_tokens AS (
```

With:
```sql
                WITH paged_tokens AS (
```

- [ ] **Step 3: Replace the CROSS JOIN line with LATERAL**

Find:
```sql
                JOIN market m ON t.token_id = m.token_id
                LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
                LEFT JOIN creator_reward cr ON t.token_id = cr.token_id AND cr.account_id = $1
                LEFT JOIN claimed_totals ctch ON t.token_id = ctch.token_id
                CROSS JOIN latest_price lp
                ORDER BY t.created_at DESC
```

Replace with:
```sql
                JOIN market m ON t.token_id = m.token_id
                LEFT JOIN balance b ON t.token_id = b.token_id AND b.account_id = $1
                LEFT JOIN creator_reward cr ON t.token_id = cr.token_id AND cr.account_id = $1
                LEFT JOIN claimed_totals ctch ON t.token_id = ctch.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp ON true
                ORDER BY t.created_at DESC
```

Note: LATERAL goes AFTER `JOIN market m` (so m.quote_id is in scope) but its exact position among the subsequent LEFT JOINs is flexible — I place it after `claimed_totals` for consistency with "enrichment joins at the end" style.

- [ ] **Step 4: cargo check**

```bash
cargo check 2>&1 | tail -10
```

- [ ] **Step 5: Commit**

```bash
git add src/controllers/token/create.rs
git commit -m "refactor(create): replace latest_price CTE with per-quote LATERAL lookup"
```

---

## Task 3: trading/market.rs — Category A with `FROM market m` (1 query)

**Files:**
- Modify: `src/controllers/trading/market.rs:62-87`

- [ ] **Step 1: Delete `WITH latest_price AS` CTE**

Replace:
```sql
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                )
                SELECT
                    m.market_type,
```

With:
```sql
                SELECT
                    m.market_type,
```

- [ ] **Step 2: Replace CROSS JOIN with LATERAL**

Replace:
```sql
                FROM market m
                JOIN token t ON m.token_id = t.token_id
                CROSS JOIN latest_price lp
                WHERE m.token_id = $1
```

With:
```sql
                FROM market m
                JOIN token t ON m.token_id = t.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp ON true
                WHERE m.token_id = $1
```

- [ ] **Step 3: cargo check + commit**

```bash
cargo check 2>&1 | tail -5
git add src/controllers/trading/market.rs
git commit -m "refactor(trading/market): replace latest_price CTE with per-quote LATERAL lookup"
```

---

## Task 4: trading/position.rs — Category A (2 queries)

**Files:**
- Modify: `src/controllers/trading/position.rs:102-121` (`get_hold_token_by_account` — `FROM balance b JOIN market m`)
- Modify: `src/controllers/trading/position.rs:230-275` (other query — `FROM token t JOIN market m`)

- [ ] **Step 1: First query (around line 102) — delete latest_price CTE**

Replace:
```sql
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                )
                SELECT
```

With:
```sql
                SELECT
```

- [ ] **Step 2: First query — replace CROSS JOIN with LATERAL**

Replace:
```sql
                FROM balance b
                ...
                JOIN market m ON b.token_id = m.token_id
                CROSS JOIN latest_price lp
```

With:
```sql
                FROM balance b
                ...
                JOIN market m ON b.token_id = m.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp ON true
```

(The `...` represents any LEFT JOINs between `balance b` and `market m` that exist in the current file — preserve them as-is. Do not reorder.)

- [ ] **Step 3: Second query (around line 230) — delete latest_price CTE**

Same removal as Step 1 but in the second query block.

- [ ] **Step 4: Second query — replace CROSS JOIN with LATERAL**

Replace:
```sql
                FROM token t
                ...
                JOIN market m ON t.token_id = m.token_id
                ...
                CROSS JOIN latest_price lp
```

With LATERAL placed immediately after `JOIN market m`:
```sql
                FROM token t
                ...
                JOIN market m ON t.token_id = m.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp ON true
                ...
```

And remove the `CROSS JOIN latest_price lp` line from further down in the FROM clause.

- [ ] **Step 5: cargo check + commit**

```bash
cargo check 2>&1 | tail -5
git add src/controllers/trading/position.rs
git commit -m "refactor(trading/position): replace latest_price CTE with per-quote LATERAL lookup"
```

---

## Task 5: trend/mod.rs — Category A (1 query)

**Files:**
- Modify: `src/controllers/trend/mod.rs:91-157`

- [ ] **Step 1: Delete latest_price CTE header**

Replace:
```sql
            WITH latest_price AS (
                SELECT price
                FROM price
                ORDER BY created_at DESC
                LIMIT 1
            )
            SELECT
```

With:
```sql
            SELECT
```

- [ ] **Step 2: Replace CROSS JOIN with LATERAL**

Replace:
```sql
            JOIN market m ON t.token_id = m.token_id
            CROSS JOIN latest_price lp
```

With:
```sql
            JOIN market m ON t.token_id = m.token_id
            LEFT JOIN LATERAL (
                SELECT p.price
                FROM price p
                WHERE p.quote_id = m.quote_id
                ORDER BY p.block_number DESC
                LIMIT 1
            ) lp ON true
```

- [ ] **Step 3: cargo check + commit**

```bash
cargo check 2>&1 | tail -5
git add src/controllers/trend/mod.rs
git commit -m "refactor(trend): replace latest_price CTE with per-quote LATERAL lookup"
```

---

## Task 6: token/order.rs — Category A (3 queries, incl. derived-table MarketCap)

**Files:**
- Modify: `src/controllers/token/order.rs:113-178` (CreationTime branch)
- Modify: `src/controllers/token/order.rs:204-269` (LatestTrade branch)
- Modify: `src/controllers/token/order.rs:295-367` (MarketCap branch with derived table aliased `m`)

This is the longest file. 3 separate queries; each must have its own CTE removal + LATERAL swap.

- [ ] **Step 1: CreationTime branch (around line 113) — delete latest_price CTE**

Replace:
```sql
                    WITH latest_price AS (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    )
                    SELECT
```

With:
```sql
                    SELECT
```

- [ ] **Step 2: CreationTime branch — replace CROSS JOIN with LATERAL**

Replace:
```sql
                    FROM market m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    CROSS JOIN latest_price lp
```

With:
```sql
                    FROM market m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN LATERAL (
                        SELECT p.price
                        FROM price p
                        WHERE p.quote_id = m.quote_id
                        ORDER BY p.block_number DESC
                        LIMIT 1
                    ) lp ON true
```

- [ ] **Step 3: LatestTrade branch — delete latest_price CTE**

Same removal as Step 1 in the LatestTrade block (around line 204).

- [ ] **Step 4: LatestTrade branch — replace CROSS JOIN with LATERAL**

Same replacement as Step 2 in the LatestTrade block (around line 269). The structure is similar.

- [ ] **Step 5: MarketCap branch — delete latest_price CTE**

Same removal in the MarketCap block (around line 295).

- [ ] **Step 6: MarketCap branch — verify derived-table `m` exposes `quote_id`**

The MarketCap branch has a derived table:
```sql
FROM (
    SELECT m.token_id, m.price, m.market_type, m.pool_id, m.quote_id, m.reserve_quote, m.reserve_token, m.volume, m.ath_price, m.ath_price_native
    FROM market m
    JOIN token t ON m.token_id = t.token_id
    WHERE {}
    ORDER BY m.price {}
    LIMIT $1 OFFSET $2
) m
JOIN token t ON m.token_id = t.token_id
```

**Verify** the inner `SELECT` lists `m.quote_id` (added in commit `af7a96d`). If missing, re-add.

- [ ] **Step 7: MarketCap branch — replace CROSS JOIN with LATERAL**

In the outer FROM after the derived table `m`:

Replace:
```sql
                    ) m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    CROSS JOIN latest_price lp
                    ORDER BY m.price {}
```

With:
```sql
                    ) m
                    JOIN token t ON m.token_id = t.token_id
                    JOIN account a ON t.creator = a.account_id
                    LEFT JOIN account_x ax ON a.account_id = ax.account_id
                    LEFT JOIN LATERAL (
                        SELECT p.price
                        FROM price p
                        WHERE p.quote_id = m.quote_id
                        ORDER BY p.block_number DESC
                        LIMIT 1
                    ) lp ON true
                    ORDER BY m.price {}
```

Note: here `m` is the derived table alias; `m.quote_id` resolves to the projected column.

- [ ] **Step 8: cargo check + commit**

```bash
cargo check 2>&1 | tail -10
git add src/controllers/token/order.rs
git commit -m "refactor(token/order): replace latest_price CTE with per-quote LATERAL lookup (3 queries)"
```

---

## Task 7: search/mod.rs — Category A (3 queries)

**Files:**
- Modify: `src/controllers/search/mod.rs:213-255` (first branch)
- Modify: `src/controllers/search/mod.rs:271-313` (second branch)
- Modify: `src/controllers/search/mod.rs:323-365` (third branch)

All 3 branches follow the same pattern: `FROM token t JOIN market m ON t.token_id = m.token_id CROSS JOIN latest_price lp`.

- [ ] **Step 1: First branch — delete CTE + swap to LATERAL**

Delete:
```sql
                    WITH latest_price AS (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    )
                    SELECT
```
→ `                    SELECT`

Replace:
```sql
                    JOIN market m ON t.token_id = m.token_id
                    CROSS JOIN latest_price lp
```
With:
```sql
                    JOIN market m ON t.token_id = m.token_id
                    LEFT JOIN LATERAL (
                        SELECT p.price
                        FROM price p
                        WHERE p.quote_id = m.quote_id
                        ORDER BY p.block_number DESC
                        LIMIT 1
                    ) lp ON true
```

- [ ] **Step 2: Second branch — same swap**

(Note: second and third branches have extra indentation — one more level. Adjust LATERAL block indentation to match.)

Delete:
```sql
                        WITH latest_price AS (
                            SELECT price
                            FROM price
                            ORDER BY created_at DESC
                            LIMIT 1
                        )
                        SELECT
```
→ `                        SELECT`

Replace:
```sql
                        JOIN market m ON t.token_id = m.token_id
                        CROSS JOIN latest_price lp
```
With:
```sql
                        JOIN market m ON t.token_id = m.token_id
                        LEFT JOIN LATERAL (
                            SELECT p.price
                            FROM price p
                            WHERE p.quote_id = m.quote_id
                            ORDER BY p.block_number DESC
                            LIMIT 1
                        ) lp ON true
```

- [ ] **Step 3: Third branch — same swap**

Same pattern as Step 2 for the third branch.

- [ ] **Step 4: cargo check + commit**

```bash
cargo check 2>&1 | tail -10
git add src/controllers/search/mod.rs
git commit -m "refactor(search): replace latest_price CTE with per-quote LATERAL lookup (3 queries)"
```

---

## Task 8: hype/mod.rs — Category A (3 queries, alias `h`)

**Files:**
- Modify: `src/controllers/hype/mod.rs:136-169` (first query)
- Modify: `src/controllers/hype/mod.rs:256-289` (second query)
- Modify: `src/controllers/hype/mod.rs:407-440` (third query)

Hype uses `h` alias for the hype table and joins market via `h.token_id = m.token_id`.

- [ ] **Step 1: First query — delete CTE + swap to LATERAL**

Delete:
```sql
                    WITH latest_price AS (
                        SELECT price
                        FROM price
                        ORDER BY created_at DESC
                        LIMIT 1
                    )
                    SELECT
```
→ `                    SELECT`

Replace:
```sql
                    JOIN market m ON h.token_id = m.token_id
                    CROSS JOIN latest_price lp
```
With:
```sql
                    JOIN market m ON h.token_id = m.token_id
                    LEFT JOIN LATERAL (
                        SELECT p.price
                        FROM price p
                        WHERE p.quote_id = m.quote_id
                        ORDER BY p.block_number DESC
                        LIMIT 1
                    ) lp ON true
```

- [ ] **Step 2: Second query — same swap**

Apply the same transformation in the second query block (around line 256).

- [ ] **Step 3: Third query — same swap**

Apply the same transformation in the third query block (around line 407).

- [ ] **Step 4: cargo check + commit**

```bash
cargo check 2>&1 | tail -10
git add src/controllers/hype/mod.rs
git commit -m "refactor(hype): replace latest_price CTE with per-quote LATERAL lookup (3 queries)"
```

---

## Task 9: trading/swap_history.rs — Category B (2 queries, add market join)

**Files:**
- Modify: `src/controllers/trading/swap_history.rs:123-175` (`get_swaps_by_account`)
- Modify: `src/controllers/trading/swap_history.rs:262-293` (`get_swaps_by_token`)

These queries use `FROM swap s` / `FROM recent_swaps rs` without joining `market`. We must ADD `JOIN market m` then swap latest_price to LATERAL.

- [ ] **Step 1: `get_swaps_by_account` — delete latest_price CTE, keep recent_swaps**

The query has two CTEs: `latest_price` and `recent_swaps`. Delete the first one.

Replace:
```sql
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                ),
                recent_swaps AS (
```

With:
```sql
                WITH recent_swaps AS (
```

- [ ] **Step 2: `get_swaps_by_account` — add `JOIN market m` and replace CROSS JOIN**

Find the outer FROM clause (after the `SELECT ... COALESCE(lp.price, 0) as native_price ...`):

Replace:
```sql
                FROM recent_swaps rs
                JOIN token t ON rs.token_id = t.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                CROSS JOIN latest_price lp
                ORDER BY rs.created_at DESC
```

With:
```sql
                FROM recent_swaps rs
                JOIN token t ON rs.token_id = t.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                JOIN market m ON rs.token_id = m.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp ON true
                ORDER BY rs.created_at DESC
```

- [ ] **Step 3: `get_swaps_by_token` — delete latest_price CTE**

This query has only `latest_price` as its CTE; removing it means the `WITH` keyword goes away entirely.

Replace:
```rust
        let mut query_sql = r#"
        WITH latest_price AS (
            SELECT price
            FROM price
            ORDER BY created_at DESC
            LIMIT 1
        )
        SELECT
```

With:
```rust
        let mut query_sql = r#"
        SELECT
```

- [ ] **Step 4: `get_swaps_by_token` — add `JOIN market m` and replace CROSS JOIN**

Replace:
```sql
        FROM swap s
        JOIN account a ON s.account_id = a.account_id
        LEFT JOIN LATERAL (
            SELECT
                ax.x_handle,
                x_image_uri
            FROM account_x ax
            WHERE ax.account_id = a.account_id
            LIMIT 1
        ) ax ON true
        CROSS JOIN latest_price lp
        WHERE s.token_id = $1"#
```

With:
```sql
        FROM swap s
        JOIN account a ON s.account_id = a.account_id
        LEFT JOIN LATERAL (
            SELECT
                ax.x_handle,
                x_image_uri
            FROM account_x ax
            WHERE ax.account_id = a.account_id
            LIMIT 1
        ) ax ON true
        JOIN market m ON s.token_id = m.token_id
        LEFT JOIN LATERAL (
            SELECT p.price
            FROM price p
            WHERE p.quote_id = m.quote_id
            ORDER BY p.block_number DESC
            LIMIT 1
        ) lp ON true
        WHERE s.token_id = $1"#
```

- [ ] **Step 5: cargo check + commit**

```bash
cargo check 2>&1 | tail -10
git add src/controllers/trading/swap_history.rs
git commit -m "refactor(trading/swap_history): add market join and per-quote LATERAL price lookup (2 queries)"
```

---

## Task 10: chester/mod.rs — Category B (1 query, add market join)

**Files:**
- Modify: `src/controllers/chester/mod.rs:291-345`

- [ ] **Step 1: Delete latest_price CTE, keep recent_swaps**

Replace:
```sql
                WITH latest_price AS (
                    SELECT price
                    FROM price
                    ORDER BY created_at DESC
                    LIMIT 1
                ),
                recent_swaps AS (
```

With:
```sql
                WITH recent_swaps AS (
```

- [ ] **Step 2: Add `JOIN market m` and replace CROSS JOIN with LATERAL**

Replace:
```sql
                FROM recent_swaps rs
                JOIN token t ON rs.token_id = t.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                CROSS JOIN latest_price lp
                ORDER BY rs.created_at DESC
```

With:
```sql
                FROM recent_swaps rs
                JOIN token t ON rs.token_id = t.token_id
                JOIN account a ON t.creator = a.account_id
                LEFT JOIN account_x ax ON a.account_id = ax.account_id
                JOIN market m ON rs.token_id = m.token_id
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = m.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp ON true
                ORDER BY rs.created_at DESC
```

- [ ] **Step 3: cargo check + commit**

```bash
cargo check 2>&1 | tail -10
git add src/controllers/chester/mod.rs
git commit -m "refactor(chester): add market join and per-quote LATERAL price lookup"
```

---

## Task 11: Final verification sweep

**Files:** (verification only)

- [ ] **Step 1: Confirm all `latest_price` references removed**

```bash
grep -rn "latest_price" /Users/gyu/project/nads-pump/api-server/src/
```

Expected: empty output (0 matches).

- [ ] **Step 2: Confirm all `CROSS JOIN lp` references removed**

```bash
grep -rn "CROSS JOIN latest_price" /Users/gyu/project/nads-pump/api-server/src/
```

Expected: empty output.

- [ ] **Step 3: Confirm expected number of LATERAL blocks added**

```bash
grep -rn "LEFT JOIN LATERAL" /Users/gyu/project/nads-pump/api-server/src/controllers/ | grep -c "p.quote_id = m.quote_id"
```

Expected: `19` (one per original latest_price query).

- [ ] **Step 4: cargo check clean**

```bash
cargo check 2>&1 | tail -20
```

Expected: only pre-existing `unused import: env` warning in `src/cors.rs:5`.

- [ ] **Step 5: cargo fmt**

```bash
cargo fmt
git diff --stat
```

If fmt made changes, commit them:

```bash
git add -u
git commit -m "style: cargo fmt"
```

- [ ] **Step 6: Push branch**

```bash
git push -u origin feat/price_quote_aware
```

- [ ] **Step 7: Create PR**

```bash
gh pr create --base v2 --title "refactor: make price lookup quote-aware (V2 multi-quote support)" --body "$(cat <<'EOF'
## Summary

Observer's V2 migration made the \`price\` table multi-quote:
- Added \`quote_id VARCHAR(42) NOT NULL DEFAULT '<WMON>'\`
- PK changed to \`(quote_id, block_number)\`
- Index \`idx_price_quote_block (quote_id, block_number DESC)\` covers per-quote latest-price lookup

The api-server's existing \`latest_price\` CTE pattern (\`SELECT price FROM price ORDER BY created_at DESC LIMIT 1\`) returns a **random** quote's most-recent price — whichever row was last inserted. This is broken for any token whose quote is not the most-recently-updated one.

This PR replaces the pattern everywhere with a per-row \`LEFT JOIN LATERAL\` scoped by \`m.quote_id\`.

## Scope

- **19 queries across 10 files**
- 15 queries in 8 files already had \`market m\` in scope — direct LATERAL swap
- 3 queries in 2 files (\`swap_history\`, \`chester\`) needed an added \`JOIN market m\` first
- Semantics: for V1 tokens (quote = WMON = MON), \`native_price\` value is unchanged. For V2 non-WMON tokens, it becomes correct instead of nonsense.

## API contract

- Response field \`native_price\` now semantically means \"this token's quote-asset / USD\" rather than \"MON/USD\".
- For V1 tokens this is identical.
- For V2 non-WMON tokens this is a fix, not a regression.
- Field name rename (\`native_price\` → \`quote_price\`) deferred to a future breaking change.

## Test plan

- [ ] cargo check clean (no new warnings)
- [ ] On dev DB with V2 schema + populated price table (multiple distinct \`quote_id\` rows):
  - [ ] EXPLAIN ANALYZE one Category A query (e.g. \`/token/metadata/:id\`) — confirm \`idx_price_quote_block\` Index Scan
  - [ ] EXPLAIN ANALYZE one Category B query (e.g. \`/swap/account/:id\`) — confirm \`JOIN market m\` + LATERAL uses the index
- [ ] V1 token response diff: call \`/token/metadata/:id\` for a WMON-quote token before/after — byte-identical
- [ ] V2 token response sanity: call same endpoint for a USDC-quote token — \`native_price\` reflects USDC/USD, not arbitrary value

## Known out-of-scope

- Renaming API field \`native_price\` → \`quote_price\` (breaking)
- Swap history price historization (currently uses current quote/USD rate, not rate at swap time — unchanged behavior)
EOF
)"
```

---

## Self-Review Checklist (run before execution)

- **Spec coverage:** Plan has tasks for all 10 files in the \"Scope / File Inventory\" section. ✓
- **Placeholder scan:** No TBD / TODO / \"similar to Task N\" — each task has full SQL snippets. ✓
- **Type consistency:** LATERAL pattern is identical across tasks (same column, same alias, same `ON true`). ✓
- **Count check:** 10 tasks for 10 files + Task 0 (branch) + Task 11 (verification) = 12 tasks total. ✓
