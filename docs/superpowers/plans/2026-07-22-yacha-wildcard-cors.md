# Yacha Wildcard CORS Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow canonical HTTPS origins on every non-apex `yacha.trade` subdomain without weakening the existing URL-shape validation or localhost development rule.

**Architecture:** Keep `src/cors.rs::is_origin_allowed` as the single policy used by CORS, cookie-authentication CSRF validation, and API-key/rate-limit bypass. Parse origins with `url::Url`, validate the canonical origin shape once, then accept either an HTTPS hostname with a non-empty label prefix ending at the exact `.yacha.trade` boundary or the existing port-qualified HTTP localhost origin.

**Tech Stack:** Rust 2024, `url::Url`, Axum HTTP types, `tower-http` CORS, Cargo test tooling.

## Global Constraints

- Allow one-level and nested HTTPS subdomains beneath `yacha.trade`.
- Do not allow the `https://yacha.trade` apex origin.
- Do not allow HTTP, custom ports, userinfo, paths, queries, fragments, empty hostname labels, trailing-dot hostnames, suffix lookalikes, or superdomains for Yacha origins.
- Preserve canonical `http://localhost:<valid-port>` development origins.
- Keep one shared origin decision for CORS, CSRF validation, and API-key/rate-limit bypass.
- Do not add environment-driven wildcard configuration, regex matching, dependencies, or unrelated refactors.

---

### Task 1: Specify and implement the trusted Yacha subdomain policy

**Files:**
- Modify: `src/cors.rs`
- Test: `src/cors.rs` (`tests::origin_allow_rules`)
- Test: `src/middleware.rs` (`tests::yacha_and_local_origins_only`)

**Interfaces:**
- Consumes: `url::Url` and the raw `Origin` header string.
- Produces: `pub(crate) fn is_origin_allowed(origin: &str) -> bool`, unchanged for existing callers in `src/cors.rs` and `src/middleware.rs`.

- [ ] **Step 1: Extend the table-driven test before implementation**

Replace the existing `cases` table in `origin_allow_rules` with the following complete policy matrix:

```rust
let cases = [
    ("https://app.yacha.trade", true),
    ("https://dev.yacha.trade", true),
    ("https://api.yacha.trade", true),
    ("https://a.b.yacha.trade", true),
    ("https://foo-bar.yacha.trade", true),
    ("https://xn--bcher-kva.yacha.trade", true),
    ("http://localhost:3000", true),
    ("http://localhost:8090", true),
    ("http://localhost:", false),
    ("http://localhost:abc", false),
    ("http://localhost:65536", false),
    ("http://localhost:3000.evil", false),
    ("http://user@localhost:3000", false),
    ("http://localhost:3000/path", false),
    ("http://localhost:3000?query", false),
    ("http://localhost:3000#fragment", false),
    ("https://yacha.trade", false),
    ("http://dev.yacha.trade", false),
    ("https://dev.yacha.trade:443", false),
    ("https://dev.yacha.trade:8443", false),
    ("https://user@dev.yacha.trade", false),
    ("https://dev.yacha.trade/", false),
    ("https://dev.yacha.trade/path", false),
    ("https://dev.yacha.trade?query", false),
    ("https://dev.yacha.trade#fragment", false),
    ("https://.yacha.trade", false),
    ("https://a..yacha.trade", false),
    ("https://foo_bar.yacha.trade", false),
    ("https://-foo.yacha.trade", false),
    ("https://foo-.yacha.trade", false),
    ("https://dev.yacha.trade.", false),
    ("https://evil-yacha.trade", false),
    ("https://yacha.trade.evil.com", false),
    ("https://dev%2eyacha.trade", false),
    ("https://dev。yacha。trade", false),
    ("https://dev.yacha.trade\\@evil.com", false),
    ("blob:https://dev.yacha.trade/id", false),
    ("null", false),
    ("https://nad.fun", false),
    ("https://app.nad.fun", false),
    ("https://nadapp.net", false),
    ("https://dev-api.nadapp.net", false),
    ("https://x.symphony.io", false),
    ("https://d111abcdef8.cloudfront.net", false),
    ("https://mm-dashboard-six.vercel.app", false),
    ("https://evil.com", false),
];

let oversized_label = format!("https://{}.yacha.trade", "a".repeat(64));
assert!(!is_origin_allowed(&oversized_label));

let oversized_hostname = format!("https://{}.yacha.trade", vec!["a".repeat(63); 4].join("."));
assert!(!is_origin_allowed(&oversized_hostname));
```

Update the middleware's representative shared-policy assertions so the wrapper expects the same wildcard behavior without duplicating the full edge-case matrix:

```rust
#[test]
fn yacha_and_local_origins_only() {
    assert!(is_allowed_origin("https://app.yacha.trade"));
    assert!(is_allowed_origin("https://dev.yacha.trade"));
    assert!(is_allowed_origin("https://a.b.yacha.trade"));
    assert!(is_allowed_origin("http://localhost:3000"));
    assert!(!is_allowed_origin("https://yacha.trade"));
    assert!(!is_allowed_origin("http://dev.yacha.trade"));
    assert!(!is_allowed_origin("https://evil-yacha.trade"));
    assert!(!is_allowed_origin("https://yacha.trade.evil.com"));
}
```

Import `get_cors`, Axum's test router types, and `tower::ServiceExt`, then add this response-level preflight test:

```rust
use super::{get_cors, is_origin_allowed};
use axum::{
    Router,
    body::Body,
    http::{
        Method, Request,
        header::{
            ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_ORIGIN,
            ACCESS_CONTROL_REQUEST_METHOD, ORIGIN,
        },
    },
    routing::get,
};
use tower::ServiceExt;

#[tokio::test]
async fn cors_layer_only_echoes_trusted_origin() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(get_cors());

    let trusted = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/")
                .header(ORIGIN, "https://dev.yacha.trade")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        trusted
            .headers()
            .get(ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|value| value.to_str().ok()),
        Some("https://dev.yacha.trade")
    );
    assert_eq!(
        trusted
            .headers()
            .get(ACCESS_CONTROL_ALLOW_CREDENTIALS)
            .and_then(|value| value.to_str().ok()),
        Some("true")
    );

    let rejected = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/")
                .header(ORIGIN, "https://evil-yacha.trade")
                .header(ACCESS_CONTROL_REQUEST_METHOD, "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert!(
        rejected
            .headers()
            .get(ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none()
    );
}
```

- [ ] **Step 2: Run the focused test and verify the red state**

Run:

```bash
cargo test cors::tests
cargo test middleware::tests::yacha_and_local_origins_only
```

Expected: both commands FAIL at `https://dev.yacha.trade` because the current implementation only trusts `https://app.yacha.trade`.

- [ ] **Step 3: Implement the minimal parsed-hostname policy**

Replace `STATIC_ORIGINS` and `is_origin_allowed` with the following implementation:

```rust
const YACHA_DOMAIN_SUFFIX: &str = ".yacha.trade";

fn is_canonical_origin(origin: &str, url: &Url) -> bool {
    url.username().is_empty()
        && url.password().is_none()
        && url.path() == "/"
        && url.query().is_none()
        && url.fragment().is_none()
        && origin == url.origin().ascii_serialization()
}

fn is_valid_dns_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    let (Some(first), Some(last)) = (bytes.first(), bytes.last()) else {
        return false;
    };

    bytes.len() <= 63
        && first.is_ascii_alphanumeric()
        && last.is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
}

fn is_yacha_subdomain_origin(url: &Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let Some(subdomains) = host.strip_suffix(YACHA_DOMAIN_SUFFIX) else {
        return false;
    };

    url.scheme() == "https"
        && url.port().is_none()
        && host.len() <= 253
        && subdomains.split('.').all(is_valid_dns_label)
}

pub(crate) fn is_origin_allowed(origin: &str) -> bool {
    let Ok(url) = Url::parse(origin) else {
        return false;
    };

    if !is_canonical_origin(origin, &url) {
        return false;
    }

    is_yacha_subdomain_origin(&url)
        || (url.scheme() == "http"
            && url.host_str() == Some("localhost")
            && url.port().is_some())
}
```

- [ ] **Step 4: Run the focused test and verify the green state**

Run:

```bash
cargo test cors::tests
cargo test middleware::tests::yacha_and_local_origins_only
```

Expected: PASS with both focused policy tests passing and no failures.

- [ ] **Step 5: Format and confirm the focused test still passes**

Run:

```bash
cargo fmt --all
cargo test cors::tests
cargo test middleware::tests::yacha_and_local_origins_only
```

Expected: formatting succeeds and the focused test remains PASS.

### Task 2: Validate, review, publish, and merge the change

**Files:**
- Verify: `src/cors.rs`
- Verify: `src/middleware.rs`
- Verify: `docs/superpowers/specs/2026-07-22-yacha-wildcard-cors-design.md`
- Verify: `docs/superpowers/plans/2026-07-22-yacha-wildcard-cors.md`

**Interfaces:**
- Consumes: the completed `is_origin_allowed` policy from Task 1.
- Produces: a reviewed commit and a squash-merged pull request targeting `dev`.

- [ ] **Step 1: Run repository validation**

Run:

```bash
cargo fmt --all -- --check
cargo test
SQLX_OFFLINE=true cargo build --release
git diff --check
```

Expected: every command exits with status 0. The build uses tracked SQLx metadata and does not connect to PostgreSQL.

- [ ] **Step 2: Review the complete change**

Inspect:

```bash
git status --short
git diff -- src/cors.rs
git diff --stat dev...HEAD
git diff dev
```

Expected: only the approved design, plan, and CORS policy/test changes are present. Review must confirm exact hostname boundaries, canonical origin enforcement, localhost preservation, and no broadened apex or port access.

- [ ] **Step 3: Commit the implementation**

Run:

```bash
git add src/cors.rs src/middleware.rs
git commit -m "fix: allow Yacha subdomain origins"
```

Expected: one implementation commit containing only the CORS implementation and its two focused policy tests.

- [ ] **Step 4: Push and create the pull request**

Run:

```bash
git push -u origin fix/yacha-wildcard-cors
gh pr create \
  --base dev \
  --head fix/yacha-wildcard-cors \
  --title "fix: allow Yacha subdomain origins" \
  --body $'## Summary\n- allow canonical HTTPS origins on all non-apex `yacha.trade` subdomains\n- preserve port-qualified HTTP localhost origins and reject lookalike or non-canonical origins\n- keep CORS, CSRF validation, and API-key/rate-limit bypass on one shared trust decision\n\n## Security\nEvery `*.yacha.trade` hostname is now inside the cookie-authentication and API-key-bypass trust boundary. The parser still rejects the apex, HTTP, explicit ports, userinfo, paths, queries, fragments, invalid DNS labels, trailing dots, suffix lookalikes, and superdomains. Origin-based API-key/rate-limit exemption remains a pre-existing spoofable trust signal and needs separate remediation because changing it would alter frontend authentication behavior.\n\n## Validation\n- [x] `cargo test cors::tests`\n- [x] `cargo test middleware::tests::yacha_and_local_origins_only`\n- [x] `cargo fmt --all -- --check`\n- [x] non-DB tests: 34 passed\n- [x] `SQLX_OFFLINE=true cargo build --release`\n- [x] `git diff --check`\n\nFull `cargo test`: 33 tests passed; 15 PostgreSQL integration tests could not start because `DATABASE_URL` is unavailable.'
```

Expected: the branch push succeeds and GitHub returns a pull request URL targeting `dev`.

- [ ] **Step 5: Verify checks and squash merge**

Run:

```bash
gh pr checks --watch
gh pr merge --squash --delete-branch
```

Expected: all configured required checks pass, the PR is squash-merged into `dev`, and the remote feature branch is deleted. If the repository has no configured checks, verify that explicitly with `gh pr view` before merging.
