# Yacha Wildcard CORS Design

## Goal

Trust every canonical HTTPS origin beneath `yacha.trade` while preserving the existing localhost development rule and rejecting lookalike, non-HTTPS, or non-origin URLs.

## Context

`src/cors.rs::is_origin_allowed` is the single origin policy used by three security-sensitive paths:

- the Tower HTTP CORS layer;
- cookie-authenticated request CSRF validation; and
- the API-key and rate-limit bypass for trusted Yacha frontends.

The current policy trusts only `https://app.yacha.trade` plus `http://localhost:<port>`. As a result, `https://dev.yacha.trade` and other controlled Yacha subdomains are rejected.

## Requirements

The policy must allow:

- `https://app.yacha.trade`;
- `https://dev.yacha.trade`;
- any other one-level Yacha subdomain such as `https://foo.yacha.trade`;
- nested subdomains such as `https://a.b.yacha.trade`; and
- the existing canonical `http://localhost:<valid-port>` development origins.

The policy must reject:

- the apex origin `https://yacha.trade`;
- all HTTP Yacha origins;
- Yacha origins with an explicit port, including explicit default port `:443`;
- origins containing userinfo, paths, queries, or fragments;
- empty-label and trailing-dot hostnames;
- suffix lookalikes such as `evil-yacha.trade`; and
- superdomains such as `yacha.trade.evil.com`.

## Design

Continue parsing origins with `url::Url`; do not match the raw header with a regex. A parsed origin is trusted as a Yacha frontend only when all of the following hold:

1. Its scheme is exactly `https`.
2. Its hostname has a non-empty, valid label prefix followed by the exact boundary `.yacha.trade`.
3. It has no explicit non-default port.
4. It has no username or password.
5. It has no path beyond the parser's root, query, or fragment.
6. The original header equals the parser's canonical origin serialization. This rejects explicit default ports and non-origin URL spellings.

Keep localhost validation separate and unchanged. Remove the redundant static `https://app.yacha.trade` allowlist because that origin is covered by the new subdomain rule.

No environment-driven wildcard or runtime regex is introduced. The trusted organizational domain remains explicit in source so changes to the authentication boundary remain reviewable.

## Request Flow

Every caller continues to invoke `is_origin_allowed`:

```text
Origin header
    -> URL parsing and canonicalization
    -> Yacha HTTPS subdomain rule OR localhost development rule
    -> shared allow/deny result
       -> CORS response headers
       -> cookie-authentication CSRF decision
       -> API-key/rate-limit bypass decision
```

Malformed or non-canonical origins return `false`; callers retain their existing logging and rejection behavior. No new fallback or permissive error path is added.

## Security Considerations

Every allowed Yacha subdomain becomes a trusted web origin for cookie-authenticated requests and bypasses the API-key/rate-limit gate. DNS ownership and deployment access for every `*.yacha.trade` hostname therefore become part of the application's trust boundary. Operators must remove unused DNS records and prevent subdomain takeover.

The hostname boundary and canonical serialization checks prevent suffix confusion. Restricting the rule to HTTPS without custom ports avoids trusting local or alternate services merely because they use a Yacha hostname.

## Testing

Extend the existing table-driven `src/cors.rs` unit test before changing the implementation. The red test must demonstrate the new trusted origins and preserve rejection cases for the apex, HTTP, explicit ports, malformed origins, lookalike domains, and URL components.

After implementation, run:

```bash
cargo test cors::tests::origin_allow_rules
cargo fmt --all -- --check
cargo test
SQLX_OFFLINE=true cargo build --release
```

No external PostgreSQL, Redis, RPC, AWS, R2, or browser integration is required for this pure origin-policy change.

## Out of Scope

- Allowing `https://yacha.trade`.
- Allowing custom ports on Yacha hosts.
- Changing localhost behavior.
- Changing HAProxy or Cloudflare configuration.
- Creating or managing DNS records.
