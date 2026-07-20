# AGENTS.md

## Project

This repository is the GIWA-facing Rust API service derived from the NADS Pump backend. It serves account, social, token, market, trade, ordering, search, and management APIs.

- Runtime: Rust 2024, Tokio, Axum, and Utoipa/OpenAPI.
- State: PostgreSQL through SQLx, one `RedisDatabase` connected through `REDIS_URL` for sessions plus trading/cache data, and in-process caches.
- Integrations: EVM access through Alloy plus AWS S3 and Rekognition.
- Active market wire/database values are `NADFUN`, `UNISWAPV3`, `V2_CURVE`, and `V2_DEX`, mapped respectively to `Curve`, `Dex`, `V2Curve`, and `V2Dex`.

## Structure

- `src/main.rs` assembles configuration, shared state, middleware, and the HTTP server.
- `src/router/` owns HTTP routing and request/response boundaries.
- `src/controllers/` and `src/services/` contain orchestration and domain behavior.
- `src/db/` contains PostgreSQL, Redis, and AWS adapters.
- `src/types/` contains domain and API types; `src/utils/` contains shared helpers.
- `src/metrics/` exposes service instrumentation.
- `migrations` is a mode-160000 submodule/gitlink for the schema repository; tracked local `.sqlx/` metadata defines this service's query contract.
- `API.md`, `TERMINAL_API.md`, and `TOKEN_CREATION_FLOW.md` document public flows.

## Commands

```bash
cargo fmt --all -- --check
cargo test
SQLX_OFFLINE=true cargo build --release
cargo run -- --port 8080
```

The release build mirrors the Docker build's SQLx offline mode. Running the service still requires its external PostgreSQL, Redis, RPC, and AWS configuration. `cargo run` is operator-only and destructive to the configured Redis server: startup unconditionally issues `FLUSHALL` through the single `REDIS_URL`, deleting every database and key on that server. Use only an explicitly authorized isolated Redis target.

## Project-Specific Rules

- Keep transport parsing in `src/router/`, domain decisions in `src/controllers/` and `src/services/`, and persistence details in `src/db/`.
- Preserve exact API field shapes and nullability; update Utoipa annotations and the relevant API document when a public response changes.
- Use `BigDecimal` for persisted or returned financial quantities; do not introduce floating-point accounting.
- Keep `NADFUN`, `UNISWAPV3`, `V2_CURVE`, and `V2_DEX` synchronized across Rust types, SQL, filters, caches, and API output.
- When schema or SQLx queries change, edit the migrations repository, intentionally update its parent gitlink, and refresh this repository's tracked local `.sqlx/` metadata.
- Treat cache-key, TTL, and invalidation changes as cross-layer changes; verify both the database source of truth and Redis behavior.
- Do not treat old stress reports, logs, or handoff documents as current behavior when source and recent history disagree.

## Security and Sensitive Operations

- Preserve authentication middleware, secure-cookie behavior, CORS restrictions, rate limits, and parameterized SQLx queries.
- Never log session identifiers, signing material, API keys, database URLs, Redis credentials, or object-store credentials.
- Validate uploaded media before storage and keep moderation failures explicit rather than silently accepting content.
- Database migrations, cache flushes, object deletion, and blockchain writes are operational actions; require explicit authorization and a rollback plan.
- Keep production addresses and endpoints in runtime configuration, not source or documentation examples.

## Validation

- Run formatting, focused tests for the changed module, and `cargo test` when the available services permit it.
- Build with `SQLX_OFFLINE=true` to catch divergence from tracked query metadata without connecting to a database.
- For route changes, verify status codes, response JSON, authentication boundaries, and OpenAPI output.
- Report any integration checks skipped because PostgreSQL, Redis, RPC, or AWS services were unavailable.
