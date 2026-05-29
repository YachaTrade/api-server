# Terminal API V2 Integration Plan

## Decision

Keep the existing Terminal API routes as the public integration surface. Do not add a `/v2` route prefix.

Existing routes stay:

- `GET /latest-block`
- `GET /asset?id=...`
- `GET /pair?id=...`
- `GET /events?fromBlock=...&toBlock=...`
- `GET /:token_address`

This checkpoint intentionally does not change router registration or runtime behavior. Implementation will follow in a later change.

## Follow-Up Scope

1. Keep `/latest-block` path unchanged.
2. Extend `/asset?id=<address>` to resolve assets from `token`, `quote_token`, and `dex_token`.
3. Extend `/pair?id=<address>` to handle launch token IDs and DEX pool IDs.
4. Remove the WMON-only assumption from pair and event asset ordering. Use `market.quote_id` for curve markets and `pool.token0` / `pool.token1` for DEX pools.
5. Extend `/events` to return a single chronological feed across launchpad `swap` / `mint` / `burn` rows and DEX `dex_swap` / `dex_mint` / `dex_burn` rows.
6. Use each asset's configured decimals when formatting event amounts and reserves.

## Compatibility

Response shapes should stay GeckoTerminal-compatible. The endpoint URLs remain stable while the backing data coverage expands to V1 and V2 markets.
