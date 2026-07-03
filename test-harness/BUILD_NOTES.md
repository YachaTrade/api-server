# BUILD_NOTES — X Hidden Creator Launch test harness

## What was built

`test-harness/xverify.html` — single-file, no-build, vanilla-JS dev page for manually
exercising the X Hidden Creator Launch flow end-to-end against a locally running
`api-server`: connect wallet (SIWE-style login) -> verify with X (OAuth) -> refresh
pending signals -> add/remove followed-by handles -> mine a salt -> reserve -> finalize
-> check `/trade/xinfo/:token_id`. A log panel records every request/response (method,
URL, request body, HTTP status, response body or fetch error). No on-chain calls are
made anywhere in the page.

## Contracts confirmed by reading source (not assumed)

- **Auth nonce/session** (`src/types/auth/mod.rs`, `src/services/auth/mod.rs`,
  `src/controllers/auth/{nonce,session}.rs`):
  - `POST /auth/nonce` request `{address}` -> response `{nonce}` — **the `nonce` field
    actually holds the full SIWE-style message text**, not a short token:
    `"Account:\n\n{address}\n\nURI: {domain}\n\nVersion: 1\n\nChain ID: {chain_id}\n\nNonce: {uuid}\n\nIssued At: {iso8601}"`.
    This is generated in `AuthService::generate_nonce` and stored verbatim in
    `auth_nonce.message`.
  - `POST /auth/session` request is `AuthSessionRequest { signature, nonce, chain_id,
    wallet_address: Option<String> }` where **`nonce` must be the exact message text
    returned above** (not a derived/short value) — `create_session` matches it against
    the stored row via `get_and_delete_nonce(address, payload.nonce)`. Signature
    recovery uses `alloy::signers::Signature::recover_address_from_msg(message)`
    (standard EIP-191 personal_sign digest over the raw message bytes).
  - Response: `AuthSessionResponse { account_info: AccountInfo }`, session cookie set
    via `Set-Cookie` with `HttpOnly; Secure; SameSite=None` (name from `COOKIE_NAME`
    env). This means the browser must treat the origin as a secure context for the
    cookie to stick — Chrome/Edge do this for `http://localhost`; Safari/Firefox may
    not. Noted on-page.
  - `chain_id` must equal the server's `CHAIN_ID` env var or the request is rejected
    with a descriptive `AppError::BadRequest` that names the required chain id. The
    page reads the connected wallet's current chain via `eth_chainId` rather than
    hardcoding a number (no safe default was discoverable without reading the
    developer's local `.env`, and the task says not to invent one).

- **CORS / Origin allowlist** (`src/cors.rs::is_origin_allowed`, `src/middleware.rs::is_allowed_origin`):
  Both allowlists are **hardcoded in source**, not env-driven, and both include
  `origin.starts_with("http://localhost:")` — i.e. **any port** on `http://localhost`
  is allowed (plus `https://nad.fun`, `https://nadapp.net`, `https://mm-dashboard-six.vercel.app`,
  and `*.nad.fun` / `*.symphony.io` / `*.cloudfront.net`). So the harness just needs to
  be served via plain `http://localhost:<any-port>`; the on-page instructions use
  `python3 -m http.server 5500` as one concrete example, not a required port.

- **X verification field names** (`src/types/x_verification/mod.rs`,
  `src/types/token/x_verification.rs`):
  - `PendingResponse { followers_count: Option<i64>, followed_by: Vec<XFollowedByEntry> }`.
  - `XFollowedByEntry { x_handle, x_image_uri, x_followers_count: i64, is_x_verified: bool }`
    — confirmed the verified-flag field name is `is_x_verified` (also asserted by an
    existing unit test in that file).
  - `FollowedByRequest { handle }`, `FollowedByResponse { is_following: bool, entry: Option<XFollowedByEntry> }`.
  - `ReserveRequest { token_id, creator, salt, version: TokenVersion }` (version has
    **no serde default** here — required field, confirmed by an existing unit test
    `reserve_request_requires_version`).
  - `FinalizeRequest { token_id }` only (confirmed by unit test
    `finalize_request_is_token_id_only`) — reservation ownership implies the rest.
  - `DELETE /x/followed-by/:handle` route exists (`XVerificationPath::FollowedByDelete`),
    session-authed, returns `PendingResponse`.

- **`TokenVersion` V2 serialized string** (`src/types/common/info.rs`): plain unit
  enum `{ V1, V2 }`, no `#[serde(rename...)]`, so it serializes/deserializes as the
  literal strings `"V1"` / `"V2"`. Confirmed by the same `reserve_request_requires_version`
  test (`{"version":"V2"}` deserializes to `TokenVersion::V2`).

- **OAuth callback error query param** (`src/router/x_verification/handler.rs`,
  `failure_redirect`): appends `x_verify_error=<code>` (using `&` instead of `?` if the
  configured failure URL already has a `?`) to the **fixed, server-env-configured**
  `X_OAUTH_REDIRECT_FAILURE_URL`. Real codes emitted by `oauth_callback`: `denied`
  (X returned an `error` param), `bad_request` (missing `code`/`state`), `state_expired`
  (`AppError::Gone`), `x_error` (any other failure). Success just does
  `Redirect::to(&X_OAUTH_REDIRECT_SUCCESS_URL)` verbatim — the server does not append
  anything on success; any query string on success is whatever the operator bakes into
  the env var itself. The page parses `x_verify_error` from `location.search` on load
  and shows it in a red banner (also logged).

- **`/token/salt`** (`src/types/token/salt.rs`, `src/router/token/handler.rs`,
  `src/router/token/mod.rs`): `POST /token/salt`, **no auth middleware** (public route).
  Request `MineSaltRequest { creator, name, symbol, metadata_uri, version: TokenVersion
  (defaults to V1 if omitted) }`. Response `MineSaltResponse { salt, address }`.
  `metadata_uri` is validated server-side against `ALLOWED_IMAGE_DOMAIN` env (default
  `https://storage.nadapp.net/`) — noted on-page since a mismatched value would produce
  a confusing 400.

- **`GET /trade/xinfo/:token_id`** (`src/router/trade/handler.rs`,
  `src/types/trading/xinfo.rs`): public, no auth. Uses `valid_account_id` (plain EIP-55
  checksum normalization) for the path param, **not** `valid_token_id` — i.e. it does
  not require the vanity-suffix convention some other token lookups enforce (per prior
  project notes on `valid_token_id` vanity trap). Response `XInfoResponse {
  x_verification: Option<TokenXVerification> }` where `TokenXVerification {
  followers_count: i64, followed_by: Vec<XFollowedByEntry> }`.

- **Router mounting**: `src/main.rs` merges all sub-routers with no path prefix, so
  every path above is used exactly as declared in each `*Path::as_str()`.

## Uncertainty / best-guesses and why

- **`chain_id` value**: `CHAIN_ID` is set in the developer's local `.env` but its value
  wasn't printed (avoided reading/echoing potential-secret-adjacent env values). Rather
  than hardcode a guess, the page pulls the connected wallet's current `eth_chainId` at
  sign time. If it's wrong, the server's own error message states the required chain id
  explicitly (`services/auth/mod.rs::create_session`), so the harness surfaces that
  verbatim in the result panel/log instead of guessing.
- **`personal_sign` hex encoding**: not explicitly covered by the task's file list, but
  required for correctness — MetaMask's `personal_sign` RPC expects the message as a
  `0x`-hex-encoded string in `params[0]` (it decodes back to the original UTF-8 bytes
  before applying the EIP-191 digest). The page hex-encodes the message via
  `TextEncoder` before calling `personal_sign`; the server-side `recover_address_from_msg`
  is given the original (non-hex) message text, which matches because MetaMask decodes
  the hex back to the same bytes before signing.
- **Base URL default `http://localhost:8080`**: this was specified explicitly in the
  task instructions as the input's default value. `.env.example` in this repo shows a
  comment suggesting a code-level fallback of `PORT=8000` if unset, and the local `.env`
  has its own `PORT` value (not read/printed). Since the input is editable and persisted
  via `localStorage`, this default is just a starting point — the on-page instructions
  don't assert a specific port as canonical.
- **`x_verified=1` success query param**: the server does *not* append this — success is
  a verbatim redirect to whatever `X_OAUTH_REDIRECT_SUCCESS_URL` is configured to. The
  on-page instructions suggest baking `?x_verified=1` into that env var's value as a
  convenience so the page can show a green success banner on return; this is presented
  as a suggested convention, not a claimed server contract.
- **`test-harness/` directory**: the task said it already existed empty; it did not
  exist in the working tree, so it was created with `mkdir -p`.

## How to run it

1. From the repo root:
   ```
   cd test-harness
   python3 -m http.server 5500
   ```
   Open `http://localhost:5500/xverify.html`. Any `http://localhost:<port>` works per
   the CORS/CSRF allowlist findings above — 5500 is just the example used in the
   on-page instructions.
2. Server env vars needed for the OAuth round trip (both `.expect()`'d in
   `src/config.rs`, so the server won't boot without them):
   ```
   X_OAUTH_REDIRECT_SUCCESS_URL=http://localhost:5500/xverify.html?x_verified=1
   X_OAUTH_REDIRECT_FAILURE_URL=http://localhost:5500/xverify.html
   ```
3. Run `api-server` locally as usual (its own `.env` already has `COOKIE_NAME`,
   `CHAIN_ID`, `ALLOWED_IMAGE_DOMAIN` etc. configured for this dev environment).
4. In the page: set Base URL (default `http://localhost:8080`, edit/persisted via
   `localStorage`) -> Connect Wallet -> Verify with X -> Refresh Pending -> Add
   Followed-By -> Mine Salt -> Reserve -> Finalize -> Check xinfo. Every request and
   response is visible in the bottom log panel regardless of success/failure.

## Verification performed (no running server needed)

- Extracted the `<script>` block and ran `node --check` against it — valid JS syntax.
- Counted opening/closing tags for `div`, `pre`, `ol`, `li`, `label`, `button`, `p`,
  `h1`, `h2`, `strong`, `em`, `code`, `script`, `style` — all balanced.
- Grepped for `<script src=`, CDN hosts, and any `http(s)://` reference outside of
  `localhost` / `nad.fun` / `nadapp.net` / `storage.nadapp.net` domains mentioned in
  prose or example values — none found. The file is fully self-contained.
- Cross-checked every fetch URL/method/body field name in the script against the Rust
  source files listed above.
