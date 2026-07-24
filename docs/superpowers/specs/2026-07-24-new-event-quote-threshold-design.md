# Quote-Aware New Event Threshold Design

## Goal

Include BUY and SELL entries in `GET /new_event` when their quote-denominated
amount is at least `0.0001` of that market's quote token, regardless of the
quote token's decimals.

## Current Behavior

The BUY and SELL queries compare raw `swap.quote_amount` values against the
hard-coded integer `1000000000000000000`. This represents one whole quote token
only when the quote token has 18 decimals. It cannot express the requested
`0.0001` threshold correctly for quote tokens with other decimal counts.

The endpoint independently selects:

- the two newest token CREATE events;
- the four newest qualifying BUY events; and
- the four newest qualifying SELL events.

It then merges those category results and orders them by their event timestamp.
This category quota, ordering, response shape, and caching behavior remain
unchanged.

## Design

The BUY and SELL queries will resolve each swap's quote token through:

```text
swap.token_id
    -> market.token_id
    -> market.quote_id
    -> quote_token.decimals
```

Each query will compare the raw on-chain `swap.quote_amount` against:

```text
ceil(0.0001 × 10^quote_token.decimals)
```

The human threshold will be supplied as an exact `BigDecimal` query parameter.
PostgreSQL will perform the power, multiplication, ceiling, and comparison with
`NUMERIC`; no floating-point accounting will be introduced.

Examples:

| Quote decimals | Minimum raw amount |
| ---: | ---: |
| 18 | 100000000000000 |
| 6 | 100 |
| 3 | 1 |

Using `CEIL` ensures that quote tokens with fewer than four decimal places still
require at least one indivisible raw unit. A market without matching
`quote_token` metadata is excluded because the service cannot safely determine
its human-denominated threshold.

## Data and API Compatibility

No public request or response field changes. The `amount` response continues to
contain the raw quote amount as a decimal string.

The following behavior remains unchanged:

- `CREATE` has no amount threshold and returns at most two rows;
- BUY and SELL each return at most four rows;
- the merged response is sorted newest first;
- Redis and in-process cache keys and TTLs remain unchanged.

## Testing

Add a PostgreSQL-backed controller test that seeds two quote tokens:

- an 18-decimal quote with swaps immediately below and exactly at
  `100000000000000`; and
- a 6-decimal quote with swaps immediately below and exactly at `100`.

The test must first fail under the existing one-token raw threshold, then pass
after the query becomes quote-aware. It will verify both BUY and SELL behavior
through the real controller query seam.

Run formatting, the focused new-event test, the available test suite, and the
SQLx offline release build. If PostgreSQL is not already available, use only a
disposable local PostgreSQL instance for the focused database test.

## Out of Scope

- Changing the CREATE/BUY/SELL result quotas.
- Changing final event ordering or tie-breaking.
- Changing the returned raw `amount` representation.
- Cache invalidation or TTL changes.
- Filtering by market type, NSFW status, chain, or token uniqueness.
