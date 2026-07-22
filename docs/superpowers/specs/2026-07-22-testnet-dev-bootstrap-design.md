# Testnet Environment And Dev Branch Bootstrap Design

## Goal

Provide a safe, reproducible GIWA testnet configuration reference and make `dev` the normal pull-request target without committing runtime credentials.

## Environment Files

- The operator-owned `.env.testnet` remains a local, ignored file in the canonical checkout.
- The repository tracks `config/testnet.env.template` instead. It contains the approved public Yacha values and `REPLACE_ME` placeholders for credentials.
- The template contains only variables used by the current GIWA API server. Retired NADS/V1/V2 product variables and environment-driven CORS variables are omitted.
- `APP_DOMAIN` and `COOKIE_NAME` are mandatory runtime settings; operators must not rely on legacy source fallbacks.
- CORS remains source-controlled: `https://app.yacha.trade` and strict `http://localhost:<port>` origins only.
- The current storage domain remains unchanged until a Yacha storage domain is supplied.

## Branch Workflow

- Normal pull requests target `dev`.
- `main` is promotion/release-only and requires explicit authorization.
- Because `dev` does not yet exist, this bootstrap change is the one-time exception: it is merged into `main`, then `dev` is created from that updated `main` commit.
- GitHub's default branch is changed to `dev` after the branch is published.
- The repository keeps squash merge as its only enabled pull-request merge method.
- Before remote writes, record the current `main` commit, default branch, and merge settings. Rollback restores the prior default branch first and reverts the bootstrap commit through a pull request; deleting `dev` requires separate confirmation if later work depends on it.

## Safety And Validation

- Never stage or print `.env.testnet`.
- Every credential field in the tracked template uses `REPLACE_ME`, including PostgreSQL userinfo.
- Confirm the runtime file is still ignored with `git check-ignore`.
- Inspect the staged template and reject credential-shaped values other than `REPLACE_ME`.
- Run `cargo fmt --all -- --check`, SQLx-offline tests against the local disposable PostgreSQL server, and `SQLX_OFFLINE=true cargo build --release`.
- Do not start the service or connect to Redis, RPC, R2, or AWS during this bootstrap.
