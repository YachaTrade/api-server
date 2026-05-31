# Branch: feat/balance-info-lp-quote

## Purpose

Add `lp_balance`, `quote_price`, and `total_balance` fields to `BalanceInfo`, and rework all 6
`BalanceInfo`-bearing endpoints from an enrich-only model to a **wallet ∪ LP union**: LP-only rows
(wallet balance = 0, LP position > 0) are now included. Sort for holdings group switches to
`total_balance DESC`. `total_count` is recomputed over the union distinct set.

## Changes

- **Task 1** (`b992e84`) — `feat(balance)`: add `total_balance` field to `BalanceInfo`
  (`balance + lp_balance`, same decimal scale); stub-computed in all 4 construction sites.

- **Task 2** (`b1361d8`) — `feat(capricorn)`: `lp_amounts_by_token` / `lp_amounts_by_owner`
  union helpers + `CAPRICORN_UNION_CAP = 2000`; EIP-55 checksummed output, uses
  `valid_account_id` (not `valid_token_id`) so legacy V1 token addresses are not dropped.

- **Task 3** (`47a7594`) — `feat(balance)`: hold-token wallet∪LP union + `total_balance DESC`
  sort + union count. `PositionService` gains `CapricornClient`; `cached_fetch_by_owner` →
  `lp_amounts_by_token` → cap → injected into SQL via `unnest`. Handler post-hoc enrichment
  removed (`get_hold_token`, `get_holdings`).

- **Task 4** (`92cb350`) — `feat(balance)`: holder wallet∪LP union + `total_balance DESC` +
  union count + cap. `cached_fetch_by_token` → `lp_amounts_by_owner` → cap. `a.account_id ASC`
  tiebreaker for deterministic pagination.

- **Task 5** (`c72595b`) — `feat(balance)`: created wallet∪LP union (keep `created_at DESC`) +
  `total_balance` + union count. `TokenCreatedService` gains `CapricornClient`.

- **Task 6** (`dc958c6`) — `feat(balance)`: gift-fee wallet∪LP union (gift sort `NULLS LAST`) +
  `total_balance` + union count. `GiftFeeService` gains `CapricornClient`.

- **Task 7** (`2773a6f`) — `refactor(balance)`: drop unused `add_token_amounts` helper + dead
  `get_total_count` public methods on `TokenCreatedController` and `GiftFeeController` (V1
  undercount trap). Remove leftover holder post-hoc enrichment block from trade handler.

### Key technical notes

- Capricorn lowercase addresses converted to EIP-55 checksum in Rust before SQL injection —
  no `LOWER()` in SQL.
- Fail-to-empty: Capricorn failure omits V1 LP rows, returns HTTP 200 (wallet∪V2 only).
- Cap: > 2000 Capricorn rows per request → V1 LP injection skipped, `tracing::warn!` logged.
- Redis response cache now holds the full union (including V1 LP). On Capricorn failure the cache
  window simply omits V1 LP rows.
- Unit assumption: `balance`, pool `reserve0/1 / total_supply × lp.balance`, and Capricorn
  `amountHuman` are all at the same decimal-adjusted scale. Verify against prod V1 tokens before
  relying on `total_balance` for V1.

## Outcome

PR: TBD
