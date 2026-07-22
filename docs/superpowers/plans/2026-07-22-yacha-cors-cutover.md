# Yacha CORS Cutover Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow only `https://app.yacha.trade` and local HTTP development origins across CORS and authentication/CSRF checks.

**Architecture:** Keep the existing CORS and middleware predicates, but replace their legacy origin rules with the same exact Yacha origin plus the existing localhost-port rule. Lock the behavior with focused unit tests before changing either predicate, then update the ignored testnet environment file's application domain.

**Tech Stack:** Rust 2024, Axum, tower-http, Cargo tests

## Global Constraints

- Allow the exact origin `https://app.yacha.trade`.
- Allow `http://localhost:<port>` for local development.
- Reject arbitrary Yacha subdomains and all legacy NADFUN/NADAPP, Symphony, CloudFront, and Vercel origins.
- Keep CORS and authentication/CSRF behavior synchronized.
- Do not commit changes unless the user separately requests a commit.

---

### Task 1: CORS predicate

**Files:**
- Modify and test: `src/cors.rs`

**Interfaces:**
- Consumes: request `Origin` values as `&str`
- Produces: `is_origin_allowed(origin: &str) -> bool`

- [ ] **Step 1: Replace the existing test cases before production code**

Use this case table in `origin_allow_rules`:

```rust
let cases = [
    ("https://app.yacha.trade", true),
    ("http://localhost:3000", true),
    ("http://localhost:8090", true),
    ("https://yacha.trade", false),
    ("https://api.yacha.trade", false),
    ("https://nad.fun", false),
    ("https://app.nad.fun", false),
    ("https://nadapp.net", false),
    ("https://dev-api.nadapp.net", false),
    ("https://x.symphony.io", false),
    ("https://d111abcdef8.cloudfront.net", false),
    ("https://mm-dashboard-six.vercel.app", false),
    ("https://evil.com", false),
];
```

- [ ] **Step 2: Verify the new CORS test fails against the legacy predicate**

Run: `cargo test cors::tests::origin_allow_rules -- --exact`

Expected: FAIL because at least `https://nad.fun` still returns `true`.

- [ ] **Step 3: Implement the minimal CORS predicate**

```rust
const STATIC_ORIGINS: [&str; 1] = ["https://app.yacha.trade"];

fn is_origin_allowed(origin: &str) -> bool {
    STATIC_ORIGINS.contains(&origin) || origin.starts_with("http://localhost:")
}
```

- [ ] **Step 4: Verify the focused CORS test passes**

Run: `cargo test cors::tests::origin_allow_rules -- --exact`

Expected: PASS.

---

### Task 2: Authentication and CSRF predicate

**Files:**
- Modify and test: `src/middleware.rs`

**Interfaces:**
- Consumes: request `Origin` values as `&str`
- Produces: `is_allowed_origin(origin: &str) -> bool`, used by authentication/CSRF checks and API-key bypass logic

- [ ] **Step 1: Replace the middleware origin test before production code**

Rename the test to `yacha_and_local_origins_only` and use these assertions:

```rust
assert!(is_allowed_origin("https://app.yacha.trade"));
assert!(is_allowed_origin("http://localhost:3000"));
assert!(is_allowed_origin("http://localhost:8090"));
assert!(!is_allowed_origin("https://yacha.trade"));
assert!(!is_allowed_origin("https://api.yacha.trade"));
assert!(!is_allowed_origin("https://nad.fun"));
assert!(!is_allowed_origin("https://app.nad.fun"));
assert!(!is_allowed_origin("https://nadapp.net"));
assert!(!is_allowed_origin("https://dev-api.nadapp.net"));
assert!(!is_allowed_origin("https://x.symphony.io"));
assert!(!is_allowed_origin("https://d111abcdef8.cloudfront.net"));
assert!(!is_allowed_origin("https://mm-dashboard-six.vercel.app"));
assert!(!is_allowed_origin("https://evil.com"));
```

- [ ] **Step 2: Verify the new middleware test fails against the legacy predicate**

Run: `cargo test middleware::tests::yacha_and_local_origins_only -- --exact`

Expected: FAIL because `https://app.yacha.trade` is not yet allowed.

- [ ] **Step 3: Implement the minimal middleware predicate and update stale comments**

```rust
fn is_allowed_origin(origin: &str) -> bool {
    origin == "https://app.yacha.trade" || origin.starts_with("http://localhost:")
}
```

Describe the API-key bypass as applying to the Yacha frontend and local development, without naming removed services.

- [ ] **Step 4: Verify the focused middleware test passes**

Run: `cargo test middleware::tests::yacha_and_local_origins_only -- --exact`

Expected: PASS.

---

### Task 3: Testnet application domain and final validation

**Files:**
- Modify: `.env.testnet`
- Verify: `src/cors.rs`, `src/middleware.rs`

**Interfaces:**
- Consumes: the existing ignored local testnet environment configuration
- Produces: authentication messages scoped to the approved Yacha frontend domain

- [ ] **Step 1: Update only the application-domain entry in `.env.testnet`**

Set the existing `APP_DOMAIN` entry to the exact approved origin `https://app.yacha.trade`. Do not print or copy any secret values from the file.

- [ ] **Step 2: Format the Rust source**

Run: `cargo fmt --all -- --check`

Expected: PASS with no output. If it fails, run `cargo fmt --all`, then repeat the check.

- [ ] **Step 3: Run all focused unit tests**

Run: `cargo test cors::tests::origin_allow_rules -- --exact`

Run: `cargo test middleware::tests::yacha_and_local_origins_only -- --exact`

Expected: both PASS.

- [ ] **Step 4: Run the full test suite and offline release build**

Run: `cargo test`

Run: `SQLX_OFFLINE=true cargo build --release`

Expected: both PASS without PostgreSQL, Redis, RPC, or AWS runtime connections.

- [ ] **Step 5: Inspect the scoped diff**

Run: `git diff --check -- src/cors.rs src/middleware.rs`

Run: `git diff -- src/cors.rs src/middleware.rs docs/superpowers/specs/2026-07-22-yacha-cors-cutover-design.md docs/superpowers/plans/2026-07-22-yacha-cors-cutover.md`

Expected: only the approved origin behavior, related comments/tests, and planning documentation are changed; `.env.testnet` remains ignored.
