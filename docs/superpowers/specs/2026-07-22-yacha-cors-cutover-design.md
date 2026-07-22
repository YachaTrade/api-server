# Yacha CORS Cutover Design

## Goal

Replace the legacy API origin allowlists with the Yacha testnet frontend origin.

## Allowed origins

- Allow the exact production frontend origin `https://app.yacha.trade`.
- Allow `http://localhost:<port>` for local development.
- Reject every legacy NADFUN/NADAPP, Symphony, CloudFront, and Vercel origin.
- Do not allow arbitrary `*.yacha.trade` subdomains.

## Implementation

- Keep the CORS predicate in `src/cors.rs` and the authentication/CSRF predicate in
  `src/middleware.rs` synchronized.
- Update both modules' focused tests before changing production predicates.
- Update `.env.testnet` so `APP_DOMAIN` is `https://app.yacha.trade`.

## Verification

- Demonstrate that the new tests fail against the old allowlists.
- Run the focused CORS and middleware tests after implementation.
- Run formatting and the full test suite, then inspect the final diff.
