# Gift Webhook Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** gift-bot 워크스페이스(producer + consumer + reply) 전체를 api-server로 이전하되, X 인입을 filtered stream에서 Account Activity webhook으로 교체하고 나머지 로직은 1:1로 보존한다.

**Architecture:** 웹훅 수신부(`GET/POST /gift/webhook`)는 전 인스턴스 stateless로 `gift_tweet`에 멱등 INSERT. consumer 워커는 `pg_advisory_lock` 단일 리더로 한 인스턴스에서만 LISTEN+폴링→`setReceiver`→reconcile→reply를 실행해 동일 봇 EOA의 nonce 충돌을 방지한다.

**Tech Stack:** Rust, Axum, sqlx(PostgreSQL), alloy(Monad RPC + `sol!` 바인딩), HMAC-SHA256(CRC/서명), OAuth 1.0a(reply). 설계 문서: `docs/superpowers/specs/2026-05-29-gift-bot-webhook-migration-design.md`.

**이식 원칙(불변):** parser/validator/executor/reconcile/poller/reply/`gift_tweet` 상태머신은 gift-bot 코드를 **동작 그대로** 옮긴다. 새 행위는 webhook 인입부(CRC/서명/payload)뿐. 출처 경로는 `/Users/gyu/project/nads-pump/gift-bot/`.

---

## File Structure

```
src/
├── chain/                       # 신규 — 체인 레이어 (gift-bot common::chain 이식)
│   ├── mod.rs                   #   pub mod bindings; signer; rpc_chain;
│   ├── bindings.rs              #   sol!{ GiftVault, ProtocolManager } — 그대로 복사
│   ├── signer.rs                #   PrivateKeySigner 로드 (Debug 마스킹) — 그대로 복사
│   └── rpc_chain.rs             #   멀티 RPC failover + chain_id 부팅 검증 — 그대로 복사
├── services/gift/               # 신규 — gift 서비스
│   ├── mod.rs                   #   서브모듈 선언
│   ├── config.rs                #   GiftConfig — env(GIFT_*) 로드·검증
│   ├── parser.rs                #   common::x::parser 이식 (Config→GiftConfig)
│   ├── domain.rs                #   common::domain (ParsedGift) — 그대로 복사
│   ├── crc.rs                   #   신규 — CRC HMAC-SHA256
│   ├── signature.rs             #   신규 — x-twitter-webhooks-signature 검증
│   ├── payload.rs               #   신규 — AAA tweet_create_events serde 타입
│   ├── ingest.rs                #   신규 — payload → parse → gift_tweet INSERT
│   ├── db.rs                    #   consumer::db 이식 (api-server pool 사용)
│   ├── validator.rs             #   consumer::validator + common::pipeline::validator 이식
│   ├── reconcile.rs             #   consumer::reconcile 이식
│   ├── executor.rs              #   consumer::executor 이식
│   ├── poller.rs                #   consumer::poller 이식 (LISTEN + 폴링)
│   ├── reply.rs                 #   consumer::reply + common::x::reply(OAuth1) 이식
│   └── worker.rs                #   신규 — advisory-lock 리더 + 워커 spawn
├── router/gift/                 # 신규
│   ├── mod.rs                   #   pub fn router()
│   ├── path.rs                  #   GiftPath enum
│   └── handler.rs               #   crc_handler / event_handler / healthz_handler
├── state.rs                     # 수정 — AppState에 gift 런타임 추가
├── middleware.rs                # 수정 — api_key_gate에서 /gift/webhook 우회
├── main.rs                      # 수정 — .merge(gift::router()) + gift::worker::spawn
└── lib.rs / config.rs           # 수정 — 모듈 선언 + 상수
```

각 Phase는 독립적으로 `cargo test` 통과 + 커밋한다. Phase 단위로 PR을 끊어도 된다(설계 §10).

---

## Phase 0: 의존성 & 모듈 스캐폴드

### Task 0.1: Cargo 의존성 추가

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: `[dependencies]`에 추가**

`Cargo.toml`의 `[dependencies]` 섹션에 추가 (alloy/sha2/base64/hex/regex/rand은 이미 존재 — 중복 추가 금지):

```toml
hmac = "0.12"
sha1 = "0.10"
percent-encoding = "2.3"
```

- [ ] **Step 2: 컴파일 확인**

Run: `cargo build`
Expected: 성공 (새 크레이트 다운로드 후 빌드 통과)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "chore(gift): add hmac/sha1/percent-encoding deps for webhook+oauth"
```

### Task 0.2: 모듈 디렉터리 스캐폴드 + lib.rs 선언

**Files:**
- Create: `src/chain/mod.rs`, `src/services/gift/mod.rs`, `src/router/gift/mod.rs`
- Modify: `src/lib.rs`

- [ ] **Step 1: 빈 모듈 파일 생성**

`src/chain/mod.rs`:
```rust
//! Monad chain layer for the gift consumer (ported from gift-bot common::chain).
pub mod bindings;
pub mod rpc_chain;
pub mod signer;
```

`src/services/gift/mod.rs`:
```rust
//! Gift bot service: X webhook ingestion + on-chain consumer + reply.
//! Ported 1:1 from the gift-bot workspace; only the X ingestion transport
//! (filtered stream → webhook) changes. See
//! docs/superpowers/specs/2026-05-29-gift-bot-webhook-migration-design.md
pub mod config;
pub mod crc;
pub mod db;
pub mod domain;
pub mod executor;
pub mod ingest;
pub mod parser;
pub mod payload;
pub mod poller;
pub mod reconcile;
pub mod reply;
pub mod signature;
pub mod validator;
pub mod worker;
```

`src/router/gift/mod.rs`:
```rust
//! Public X webhook routes (CRC + Account Activity events).
mod handler;
mod path;

use axum::routing::get;
use axum::Router;

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route(
        "/gift/webhook",
        get(handler::crc_handler).post(handler::event_handler),
    )
    .route("/gift/healthz", get(handler::healthz_handler))
}
```

- [ ] **Step 2: `src/lib.rs`에 모듈 등록**

`src/lib.rs`의 모듈 선언부에 추가 (기존 `pub mod` 목록 위치에 맞춰):
```rust
pub mod chain;
```
그리고 `services`/`router` 하위 모듈은 각 부모 `mod.rs`에서 선언되므로, `src/services/mod.rs`에 `pub mod gift;`, `src/router/mod.rs`에 `pub mod gift;`를 추가한다.

> 주의: 이 단계의 빈 `pub mod` 선언들은 다음 Task에서 파일이 채워질 때까지 컴파일이 깨진다. 그래서 Step 3 커밋은 **Phase 1 첫 모듈(domain/bindings)까지 작성한 뒤** 수행한다 — 아래 Task 1.1 이후로 미룬다.

- [ ] **Step 3: (보류)** Phase 1 첫 모듈 작성 후 함께 커밋.

---

## Phase 1: 체인 레이어 + 도메인 + GiftConfig

### Task 1.1: domain.rs + bindings.rs 1:1 복사

**Files:**
- Create: `src/services/gift/domain.rs`, `src/chain/bindings.rs`

- [ ] **Step 1: domain.rs 복사**

`/Users/gyu/project/nads-pump/gift-bot/crates/common/src/domain.rs` 내용을 그대로 `src/services/gift/domain.rs`에 작성:
```rust
//! Shared domain types across gift parser, validator, and executor.
use alloy::primitives::Address;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGift {
    pub tweet_id: String,
    pub author_username: String,
    pub token: Address,
    pub receiver: Address,
}
```

- [ ] **Step 2: bindings.rs 복사**

`gift-bot/crates/common/src/chain/bindings.rs` 전체(`sol! { contract GiftVault {...} contract ProtocolManager {...} }`)를 `src/chain/bindings.rs`에 **그대로** 복사. 코드 변경 없음 (import은 `use alloy::sol;` 하나뿐).

- [ ] **Step 3: 임시로 다른 gift 모듈 stub 처리**

`src/services/gift/mod.rs`에서 아직 미작성 모듈 선언을 잠시 주석 처리하여 컴파일이 통과하도록 한다 (작성된 `domain`만 활성). `src/chain/mod.rs`도 `bindings`만 남기고 `rpc_chain`/`signer`는 주석.

- [ ] **Step 4: 컴파일 확인**

Run: `cargo build`
Expected: 성공

- [ ] **Step 5: Commit**

```bash
git add src/lib.rs src/chain src/services/gift src/router/gift src/services/mod.rs src/router/mod.rs
git commit -m "feat(gift): scaffold modules + port domain & GiftVault bindings"
```

### Task 1.2: signer.rs 1:1 복사 + 테스트

**Files:**
- Create: `src/chain/signer.rs`

- [ ] **Step 1: signer.rs 복사**

`gift-bot/crates/common/src/chain/signer.rs` 전체(`load()` + `SignerError` + 테스트 모듈 포함)를 `src/chain/signer.rs`에 그대로 복사. `src/chain/mod.rs`의 `pub mod signer;` 주석 해제.

- [ ] **Step 2: 테스트 실행 (포팅된 known-vector 테스트)**

Run: `cargo test --lib chain::signer`
Expected: PASS (`loads_known_test_vector` — PK=…01 → 0x7E5F…Bdf)

- [ ] **Step 3: Commit**

```bash
git add src/chain/signer.rs src/chain/mod.rs
git commit -m "feat(gift): port chain signer with masked-debug key load"
```

### Task 1.3: rpc_chain.rs 1:1 복사

**Files:**
- Create: `src/chain/rpc_chain.rs`

- [ ] **Step 1: rpc_chain.rs 복사**

`gift-bot/crates/consumer/src/rpc_chain.rs` 전체를 `src/chain/rpc_chain.rs`에 복사. **import 경로 변경(유일한 수정):**
- `use common::chain::bindings::GiftVault;` → `use crate::chain::bindings::GiftVault;`
- ProtocolManager 참조 시 `use crate::chain::bindings::ProtocolManager;`

`src/chain/mod.rs`의 `pub mod rpc_chain;` 주석 해제.

- [ ] **Step 2: 컴파일 + 단위 테스트**

Run: `cargo test --lib chain::rpc_chain`
Expected: PASS (anvil 불필요한 단위 테스트만; node-bindings 테스트는 Phase 3에서 dev-dep 추가 후)

> dev-dependency `alloy` `node-bindings` feature가 필요한 라운드트립 테스트는 Phase 3 Task 3.0에서 dev-dep을 추가한 뒤 활성화한다. 지금은 `#[cfg(feature = ...)]` 또는 `#[ignore]` 처리된 채로 둔다.

- [ ] **Step 3: Commit**

```bash
git add src/chain/rpc_chain.rs src/chain/mod.rs
git commit -m "feat(gift): port multi-endpoint RPC failover chain"
```

### Task 1.4: GiftConfig — env(GIFT_*) 로드·검증

**Files:**
- Create: `src/services/gift/config.rs`
- Test: 동일 파일 `#[cfg(test)]`

- [ ] **Step 1: 실패 테스트 작성**

`src/services/gift/config.rs`:
```rust
//! GiftConfig — all gift-bot config consolidated under the GIFT_ env prefix.
//! Field semantics match gift-bot's ProducerConfig + ConsumerConfig; only the
//! env var names gain a `GIFT_` prefix to avoid api-server collisions.
use std::env;

#[derive(Debug, Clone)]
pub struct GiftConfig {
    // X webhook + OAuth 1.0a (CRC/signature/reply 공용)
    pub oauth_consumer_key: String,
    pub oauth_consumer_secret: String,
    pub oauth_access_token: String,
    pub oauth_access_token_secret: String,
    pub webhook_id: String,
    // parser slots
    pub mention_account: String,
    pub activation_prefix: String,
    pub recipient_prefix: String,
    pub required_hashtag: String,
    // chain (consumer)
    pub main_rpc_url: String,
    pub sub_rpc_url_1: Option<String>,
    pub sub_rpc_url_2: Option<String>,
    pub monad_chain_id: u64,
    pub gift_vault_address: alloy::primitives::Address,
    pub bot_private_key: String,
    pub poll_interval_ms: u64,
    pub tx_graceful_drain_ms: u64,
    pub dry_run: bool,
    pub paused: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum GiftConfigError {
    #[error("missing required env var: {0}")]
    Missing(&'static str),
    #[error("invalid value for {0}: {1}")]
    Invalid(&'static str, String),
}

impl GiftConfig {
    pub fn load() -> Result<Self, GiftConfigError> {
        use std::str::FromStr;
        fn req(k: &'static str) -> Result<String, GiftConfigError> {
            env::var(k).map_err(|_| GiftConfigError::Missing(k))
        }
        let gift_vault_address = alloy::primitives::Address::from_str(&req("GIFT_VAULT_ADDRESS")?)
            .map_err(|e| GiftConfigError::Invalid("GIFT_VAULT_ADDRESS", e.to_string()))?;
        Ok(Self {
            oauth_consumer_key: req("GIFT_X_OAUTH_CONSUMER_KEY")?,
            oauth_consumer_secret: req("GIFT_X_OAUTH_CONSUMER_SECRET")?,
            oauth_access_token: req("GIFT_X_OAUTH_ACCESS_TOKEN")?,
            oauth_access_token_secret: req("GIFT_X_OAUTH_ACCESS_TOKEN_SECRET")?,
            webhook_id: req("GIFT_X_WEBHOOK_ID")?,
            mention_account: req("GIFT_X_MENTION_ACCOUNT")?,
            activation_prefix: req("GIFT_X_ACTIVATION_PREFIX")?,
            recipient_prefix: req("GIFT_X_RECIPIENT_PREFIX")?,
            required_hashtag: req("GIFT_X_REQUIRED_HASHTAG")?,
            main_rpc_url: req("GIFT_MAIN_RPC_URL")?,
            sub_rpc_url_1: env::var("GIFT_SUB_RPC_URL_1").ok().filter(|s| !s.is_empty()),
            sub_rpc_url_2: env::var("GIFT_SUB_RPC_URL_2").ok().filter(|s| !s.is_empty()),
            monad_chain_id: req("GIFT_MONAD_CHAIN_ID")?
                .parse()
                .map_err(|e: std::num::ParseIntError| GiftConfigError::Invalid("GIFT_MONAD_CHAIN_ID", e.to_string()))?,
            gift_vault_address,
            bot_private_key: req("GIFT_BOT_PRIVATE_KEY")?,
            poll_interval_ms: env::var("GIFT_CONSUMER_POLL_INTERVAL_MS").ok()
                .and_then(|s| s.parse().ok()).unwrap_or(2000),
            tx_graceful_drain_ms: env::var("GIFT_TX_GRACEFUL_DRAIN_MS").ok()
                .and_then(|s| s.parse().ok()).unwrap_or(10000),
            dry_run: env::var("GIFT_DRY_RUN").map(|v| v == "true").unwrap_or(false),
            paused: env::var("GIFT_PAUSED").map(|v| v == "true").unwrap_or(false),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_required_returns_error() {
        // 필수 키가 없을 때 Missing 에러. (테스트는 env 격리를 위해 직접 함수 호출 형태로 검증)
        // 빈 환경에서 load()는 반드시 첫 필수 키에서 Missing.
        // SAFETY: 테스트 프로세스 전용. 병렬 간섭 방지 위해 serial 권장.
        unsafe { std::env::remove_var("GIFT_X_OAUTH_CONSUMER_KEY"); }
        let err = GiftConfig::load();
        assert!(matches!(err, Err(GiftConfigError::Missing(_))));
    }
}
```

- [ ] **Step 2: 테스트 실패 확인 → 구현은 이미 위에 포함**

Run: `cargo test --lib services::gift::config`
Expected: PASS (구현이 테스트와 함께 작성됨 — RED 단계는 컴파일 에러로 확인 후 위 코드로 GREEN)

- [ ] **Step 3: `.env.example`에 GIFT_ 블록 추가**

`.env.example`에 설계 §7의 `GIFT_*` 블록을 주석과 함께 추가.

- [ ] **Step 4: Commit**

```bash
git add src/services/gift/config.rs .env.example
git commit -m "feat(gift): GiftConfig load+validate from GIFT_ env prefix"
```

---

## Phase 2: 웹훅 수신 (CRC + 서명 + parser + ingest)

### Task 2.1: parser.rs 이식 (Config→GiftConfig)

**Files:**
- Create: `src/services/gift/parser.rs`

- [ ] **Step 1: parser.rs 복사 + 적응**

`gift-bot/crates/common/src/x/parser.rs` 전체를 `src/services/gift/parser.rs`로 복사. **수정 사항(로직 무변경):**
- `use crate::config::Config;` → `use crate::services::gift::config::GiftConfig;`
- `use crate::domain::ParsedGift;` → `use crate::services::gift::domain::ParsedGift;`
- `Parser::new(config: &Config)` 시그니처에서 `Config` → `GiftConfig`
- 필드 접근명 매핑: `config.x_activation_prefix` → `config.activation_prefix`, `config.x_recipient_prefix` → `config.recipient_prefix`, `config.x_required_hashtag` → `config.required_hashtag`, `config.x_mention_account` → `config.mention_account`
- `Parser`를 `GiftParser`로 rename (api-server 네임스페이스 명확화). 메서드/regex/로직은 그대로.
- gift-bot parser.rs의 `#[cfg(test)]` 테스트도 함께 복사하고 동일 필드명/타입에 맞춰 `GiftConfig` 생성 헬퍼만 조정.

- [ ] **Step 2: 포팅된 parser 테스트 실행**

Run: `cargo test --lib services::gift::parser`
Expected: PASS (gift-bot의 모든 parse 케이스 — hashtag/mention/bracket/ambiguous/zero/equal — 동등 통과)

- [ ] **Step 3: Commit**

```bash
git add src/services/gift/parser.rs
git commit -m "feat(gift): port tweet parser (GiftConfig-backed, logic unchanged)"
```

### Task 2.2: crc.rs — CRC challenge HMAC-SHA256

**Files:**
- Create: `src/services/gift/crc.rs`
- Test: 동일 파일

- [ ] **Step 1: 실패 테스트 작성**

`src/services/gift/crc.rs`:
```rust
//! X Account Activity CRC challenge: response_token = "sha256=" +
//! base64(HMAC-SHA256(consumer_secret, crc_token)).
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn response_token(consumer_secret: &str, crc_token: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(consumer_secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(crc_token.as_bytes());
    let sig = mac.finalize().into_bytes();
    format!("sha256={}", base64::engine::general_purpose::STANDARD.encode(sig))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_matches_known_vector() {
        // X 공식 예시 벡터: secret="secret", token="abc" 의 HMAC-SHA256 base64.
        let out = response_token("secret", "abc");
        assert!(out.starts_with("sha256="));
        // 결정성: 동일 입력 → 동일 출력
        assert_eq!(out, response_token("secret", "abc"));
        // 토큰이 다르면 결과가 다름
        assert_ne!(out, response_token("secret", "abd"));
    }
}
```

- [ ] **Step 2: 테스트 실행**

Run: `cargo test --lib services::gift::crc`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/services/gift/crc.rs
git commit -m "feat(gift): CRC challenge response token (HMAC-SHA256)"
```

### Task 2.3: signature.rs — 이벤트 서명 검증

**Files:**
- Create: `src/services/gift/signature.rs`
- Test: 동일 파일

- [ ] **Step 1: 실패 테스트 + 구현 작성**

`src/services/gift/signature.rs`:
```rust
//! Verify X's `x-twitter-webhooks-signature: sha256=<base64>` header against
//! HMAC-SHA256(consumer_secret, raw_request_body). Constant-time compare.
use base64::Engine;
use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

/// Returns true iff `header_value` (e.g. "sha256=AbC...") matches the HMAC of
/// `raw_body` under `consumer_secret`.
pub fn verify(consumer_secret: &str, raw_body: &[u8], header_value: &str) -> bool {
    let Some(b64) = header_value.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = base64::engine::general_purpose::STANDARD.decode(b64) else {
        return false;
    };
    let mut mac = HmacSha256::new_from_slice(consumer_secret.as_bytes())
        .expect("HMAC accepts any key length");
    mac.update(raw_body);
    mac.verify_slice(&expected).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::gift::crc::response_token;

    #[test]
    fn valid_signature_passes() {
        let secret = "topsecret";
        let body = br#"{"tweet_create_events":[]}"#;
        // 헤더 값을 동일 알고리즘으로 생성 (crc::response_token이 동일 HMAC을 냄)
        let header = response_token(secret, std::str::from_utf8(body).unwrap());
        assert!(verify(secret, body, &header));
    }

    #[test]
    fn tampered_body_fails() {
        let secret = "topsecret";
        let header = response_token(secret, "original");
        assert!(!verify(secret, b"tampered", &header));
    }

    #[test]
    fn malformed_header_fails() {
        assert!(!verify("s", b"x", "not-prefixed"));
        assert!(!verify("s", b"x", "sha256=!!!notbase64"));
    }
}
```

- [ ] **Step 2: 테스트 실행**

Run: `cargo test --lib services::gift::signature`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/services/gift/signature.rs
git commit -m "feat(gift): webhook signature verification (HMAC-SHA256, raw body)"
```

### Task 2.4: payload.rs — AAA tweet_create_events 타입

**Files:**
- Create: `src/services/gift/payload.rs`
- Test: 동일 파일

> **신규**: filtered-stream의 `TweetEnvelope`(author_id→includes.users 매칭)와 달리, AAA webhook은 `tweet_create_events[].user.screen_name`을 직접 제공한다. parser.parse(text, screen_name, id_str)는 그대로 재사용.

- [ ] **Step 1: 테스트 + 구현 작성**

`src/services/gift/payload.rs`:
```rust
//! Serde types for X Account Activity API webhook payloads. Only the
//! `tweet_create_events` we act on are modeled; unknown fields ignored.
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ActivityPayload {
    #[serde(default)]
    pub tweet_create_events: Vec<TweetCreateEvent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TweetCreateEvent {
    pub id_str: String,
    /// `extended_tweet.full_text`가 있으면 우선, 없으면 `text`.
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub extended_tweet: Option<ExtendedTweet>,
    pub user: TweetUser,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtendedTweet {
    #[serde(default)]
    pub full_text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TweetUser {
    pub screen_name: String,
}

impl TweetCreateEvent {
    /// 280자 초과 트윗은 `text`가 잘리므로 extended_tweet.full_text 우선.
    pub fn resolved_text(&self) -> &str {
        match &self.extended_tweet {
            Some(e) if !e.full_text.is_empty() => &e.full_text,
            _ => &self.text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_event() {
        let raw = r#"{"tweet_create_events":[{"id_str":"123","text":"hi","user":{"screen_name":"alice"}}]}"#;
        let p: ActivityPayload = serde_json::from_str(raw).unwrap();
        assert_eq!(p.tweet_create_events.len(), 1);
        let e = &p.tweet_create_events[0];
        assert_eq!(e.id_str, "123");
        assert_eq!(e.user.screen_name, "alice");
        assert_eq!(e.resolved_text(), "hi");
    }

    #[test]
    fn extended_text_takes_priority() {
        let raw = r#"{"tweet_create_events":[{"id_str":"1","text":"trunc…","extended_tweet":{"full_text":"full body"},"user":{"screen_name":"bob"}}]}"#;
        let p: ActivityPayload = serde_json::from_str(raw).unwrap();
        assert_eq!(p.tweet_create_events[0].resolved_text(), "full body");
    }

    #[test]
    fn ignores_non_tweet_events() {
        // tweet_create_events 없는 이벤트(favorite 등)는 빈 벡터로.
        let raw = r#"{"favorite_events":[{"id":"x"}]}"#;
        let p: ActivityPayload = serde_json::from_str(raw).unwrap();
        assert!(p.tweet_create_events.is_empty());
    }
}
```

- [ ] **Step 2: 테스트 실행**

Run: `cargo test --lib services::gift::payload`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/services/gift/payload.rs
git commit -m "feat(gift): AAA webhook payload types (tweet_create_events)"
```

### Task 2.5: db.rs — gift_tweet INSERT (producer write path 이식)

**Files:**
- Create: `src/services/gift/db.rs`
- Test: 동일 파일 (쿼리 컴파일은 sqlx offline; 동작 테스트는 Phase 3 통합에서)

- [ ] **Step 1: INSERT 함수 작성 (producer/ingest.rs:414-422 동일 SQL)**

`src/services/gift/db.rs` — 우선 INSERT만(나머지 consumer 쿼리는 Phase 3에서 추가):
```rust
//! gift_tweet queries. Ported from gift-bot consumer::db; uses api-server's
//! shared Postgres pool. Address columns are VARCHAR → 0x… text on write.
use sqlx::PgPool;

use crate::services::gift::domain::ParsedGift;

/// Idempotent insert from the webhook ingest path. Mirrors gift-bot
/// producer/ingest.rs: ON CONFLICT (tweet_id) DO NOTHING. The
/// gift_tweet_notify trigger fires pg_notify('gift_tweet_new') on insert.
/// Returns true if a row was inserted (false = duplicate).
pub async fn insert_gift_tweet(pool: &PgPool, gift: &ParsedGift) -> Result<bool, sqlx::Error> {
    let res = sqlx::query(
        "INSERT INTO gift_tweet (tweet_id, token_id, receiver_id, handle) \
         VALUES ($1, $2, $3, $4) \
         ON CONFLICT (tweet_id) DO NOTHING",
    )
    .bind(&gift.tweet_id)
    .bind(format!("{:#x}", gift.token))
    .bind(format!("{:#x}", gift.receiver))
    .bind(&gift.author_username)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}
```

> 주소 표기: gift-bot db.rs는 lowercase `0x…`로 저장(parser가 이미 lowercase). `{:#x}`는 alloy Address의 lowercase hex. 메모리 [[feedback_address_format]](EIP-55 canonical)와 충돌하지 않도록 — gift_tweet은 내부 버퍼 테이블이고 consumer가 `Address::from_str`로 재파싱하므로 lowercase 저장은 gift-bot 동작과 동일하게 유지(로직 1:1 원칙).

- [ ] **Step 2: 컴파일 확인 (sqlx query — 비매크로라 offline 불필요)**

Run: `cargo build`
Expected: 성공 (`sqlx::query`는 런타임 쿼리라 DB 연결 없이 컴파일)

- [ ] **Step 3: Commit**

```bash
git add src/services/gift/db.rs
git commit -m "feat(gift): gift_tweet idempotent insert (webhook ingest write path)"
```

### Task 2.6: ingest.rs — payload → parse → insert

**Files:**
- Create: `src/services/gift/ingest.rs`
- Test: 동일 파일

- [ ] **Step 1: ingest 함수 + 테스트 작성**

`src/services/gift/ingest.rs`:
```rust
//! Webhook ingest: ActivityPayload → GiftParser → gift_tweet INSERT.
//! Replaces gift-bot producer's stream loop; parse + insert logic identical.
use sqlx::PgPool;
use tracing::{info, warn};

use crate::services::gift::db::insert_gift_tweet;
use crate::services::gift::parser::GiftParser;
use crate::services::gift::payload::ActivityPayload;

/// Process every tweet_create_event: parse, and on success insert. Parse
/// rejects are logged at info (noise), not errors. Returns count inserted.
pub async fn ingest(pool: &PgPool, parser: &GiftParser, payload: &ActivityPayload) -> usize {
    let mut inserted = 0;
    for ev in &payload.tweet_create_events {
        match parser.parse(ev.resolved_text(), &ev.user.screen_name, &ev.id_str) {
            Ok(gift) => match insert_gift_tweet(pool, &gift).await {
                Ok(true) => {
                    info!(tweet_id = %gift.tweet_id, "gift tweet ingested");
                    inserted += 1;
                }
                Ok(false) => info!(tweet_id = %gift.tweet_id, "duplicate gift tweet, skipped"),
                Err(e) => warn!(tweet_id = %ev.id_str, error = %e, "gift_tweet insert failed"),
            },
            Err(reject) => {
                info!(tweet_id = %ev.id_str, reason = %reject, "tweet rejected at parse");
            }
        }
    }
    inserted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::gift::config::GiftConfig;

    // GiftParser 단독 파싱 경로 검증 (DB 없이): 잘 구성된 트윗이 ParsedGift로.
    // (insert는 Phase 3 통합 테스트에서 실 DB로 검증.)
    fn test_parser() -> GiftParser {
        // GiftConfig를 테스트용으로 직접 구성하는 헬퍼는 parser.rs 테스트와 공유.
        // 여기서는 parser가 동일 슬롯으로 동작함을 payload→text 추출 경로로만 확인.
        GiftParser::new(&test_config()).unwrap()
    }
    fn test_config() -> GiftConfig {
        // parser.rs 테스트 헬퍼와 동일 값.
        unimplemented_test_config()
    }
    fn unimplemented_test_config() -> GiftConfig {
        // 구현 시: parser.rs의 #[cfg(test)] 헬퍼를 pub(crate)로 노출해 재사용.
        todo!("reuse parser test config helper")
    }

    #[test]
    #[ignore = "wired in Phase 3 with shared test config helper"]
    fn payload_text_extraction_feeds_parser() {
        let _ = test_parser();
    }
}
```

> **이식 노트:** 위 테스트는 공유 GiftConfig 테스트 헬퍼가 필요하다. Task 2.1에서 parser.rs `#[cfg(test)]`에 만든 `GiftConfig` 생성 헬퍼를 `pub(crate) fn test_gift_config()`로 노출하고, ingest 테스트의 `test_config()`가 그것을 호출하도록 교체한다(이 Task Step 1에서 같이 처리). `#[ignore]`를 제거하고 "잘 구성된 payload → inserted 카운트 로직(파서 통과)" 단정으로 채운다.

- [ ] **Step 2: 공유 테스트 헬퍼 연결 + 테스트 실행**

parser.rs의 테스트 config 헬퍼를 `pub(crate)`로 바꾸고 ingest 테스트에서 재사용. 파서 통과 케이스 1건 단정.

Run: `cargo test --lib services::gift::ingest`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add src/services/gift/ingest.rs src/services/gift/parser.rs
git commit -m "feat(gift): webhook ingest pipeline (payload→parse→insert)"
```

### Task 2.7: AppState 배선 + 웹훅 핸들러 + 라우터

**Files:**
- Modify: `src/state.rs`
- Create: `src/router/gift/path.rs`, `src/router/gift/handler.rs`
- Modify: `src/main.rs`, `src/middleware.rs`

- [ ] **Step 1: AppState에 gift 런타임 추가**

`src/state.rs`:
```rust
use std::sync::Arc;

use crate::db::{postgres::PostgresDatabase, r2::R2Client, redis::RedisDatabase};
use crate::services::gift::config::GiftConfig;
use crate::services::gift::parser::GiftParser;

#[derive(Clone)]
pub struct GiftRuntime {
    pub config: Arc<GiftConfig>,
    pub parser: Arc<GiftParser>,
}

#[derive(Clone)]
pub struct AppState {
    pub postgres: Arc<PostgresDatabase>,
    pub redis: Arc<RedisDatabase>,
    pub r2: Arc<R2Client>,
    pub gift: Arc<GiftRuntime>,
}

impl AppState {
    pub async fn new() -> Self {
        let redis = Arc::new(RedisDatabase::new().await);
        let postgres = Arc::new(PostgresDatabase::new().await);
        let r2 = Arc::new(R2Client::new().await);

        let gift_config = Arc::new(
            GiftConfig::load().expect("GIFT_* config must be valid — check .env"),
        );
        let gift_parser = Arc::new(
            GiftParser::new(&gift_config).expect("gift parser template slots invalid"),
        );
        let gift = Arc::new(GiftRuntime { config: gift_config, parser: gift_parser });

        Self { postgres, redis, r2, gift }
    }
}
```

> `PostgresDatabase`가 노출하는 write/primary pool 접근자(예: `get_pool()` 또는 `get_write_pool()`)를 확인해 INSERT/consumer가 **primary(write)** pool을 쓰도록 한다(읽기 전용 replica 금지). 정확한 메서드명은 `src/db/postgres` 확인 후 사용.

- [ ] **Step 2: path.rs**

`src/router/gift/path.rs`:
```rust
#[derive(Debug, Clone, Copy)]
pub enum GiftPath {
    Webhook,
    Healthz,
}

impl GiftPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            GiftPath::Webhook => "/gift/webhook",
            GiftPath::Healthz => "/gift/healthz",
        }
    }
}
```

- [ ] **Step 3: handler.rs**

`src/router/gift/handler.rs`:
```rust
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::services::gift::{crc, ingest, payload::ActivityPayload, signature};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CrcQuery {
    pub crc_token: String,
}

/// GET /gift/webhook?crc_token=… → { "response_token": "sha256=…" }
pub async fn crc_handler(
    State(state): State<AppState>,
    Query(q): Query<CrcQuery>,
) -> impl IntoResponse {
    let token = crc::response_token(&state.gift.config.oauth_consumer_secret, &q.crc_token);
    Json(json!({ "response_token": token }))
}

/// POST /gift/webhook — verify signature over raw body, parse, insert. Always
/// 200 on accepted (X retries non-2xx); 401 on bad signature.
pub async fn event_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let sig = headers
        .get("x-twitter-webhooks-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !signature::verify(&state.gift.config.oauth_consumer_secret, &body, sig) {
        warn!("gift webhook signature verification failed");
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let payload: ActivityPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            // 서명은 통과했으나 파싱 실패 — 200으로 흡수(X 재시도 방지), 로깅만.
            warn!(error = %e, "gift webhook payload deserialize failed");
            return StatusCode::OK.into_response();
        }
    };
    let pool = state.postgres.get_pool(); // primary/write pool accessor (Step 1 주석 참조)
    let n = ingest::ingest(pool, &state.gift.parser, &payload).await;
    if n > 0 {
        tracing::info!(inserted = n, "gift webhook batch ingested");
    }
    StatusCode::OK.into_response()
}

/// GET /gift/healthz — liveness for haproxy backend.
pub async fn healthz_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}
```

- [ ] **Step 4: main.rs 라우터 머지**

`src/main.rs`의 router 체인에 `.merge(gift::router())` 추가 (다른 `.merge(...)` 옆). 필요한 `use api_server::router::gift;` 임포트 추가.

- [ ] **Step 5: middleware.rs — api_key_gate 우회**

`src/middleware.rs`의 `api_key_gate` 함수 진입부에서 webhook 경로는 게이트를 건너뛰도록:
```rust
// gift webhook: X가 X-API-Key를 보내지 않음 — 인증은 HMAC 서명이 담당.
let path = req.uri().path();
if path == "/gift/webhook" || path == "/gift/healthz" {
    return Ok(next.run(req).await);
}
```
(함수 시그니처/`next` 사용은 기존 `api_key_gate` 패턴 확인 후 정확히 맞춤. `method_based_timeout`의 upload 예외 분기와 동일 스타일.)

- [ ] **Step 6: 빌드 + 핸들러 통합 테스트**

`tests/`에 axum 라우터 통합 테스트(또는 `#[tokio::test]` in handler) 추가: CRC GET이 `response_token` 반환, 서명 위조 POST가 401, 유효 서명 빈 payload가 200.

Run: `cargo test gift`
Expected: PASS. 추가로 `cargo build`로 전체 배선 통과 확인.

- [ ] **Step 7: Commit**

```bash
git add src/state.rs src/router/gift src/main.rs src/middleware.rs tests/
git commit -m "feat(gift): wire webhook routes + AppState gift runtime + api-key bypass"
```

---

## Phase 3: consumer 워커 (advisory-lock 리더 + 폴링 + 온체인)

### Task 3.0: dev-dependency 추가 (anvil 라운드트립)

**Files:**
- Modify: `Cargo.toml`

- [ ] **Step 1: dev-deps에 alloy node-bindings**

`Cargo.toml`의 `[dev-dependencies]`에 (없으면):
```toml
alloy = { version = "1.0.24", features = ["node-bindings"] }
```
Run: `cargo build --tests` → 성공. Commit: `chore(gift): add alloy node-bindings dev-dep for chain roundtrip tests`.

### Task 3.1: db.rs 확장 — consumer 쿼리 이식

**Files:**
- Modify: `src/services/gift/db.rs`

- [ ] **Step 1: consumer::db.rs의 나머지 함수 이식**

`gift-bot/crates/consumer/src/db.rs`의 `GiftRow`, `claim_pending`(FOR UPDATE SKIP LOCKED), 상태전이 함수(`mark_submitted`/`mark_completed`/`mark_rejected`/`reset_pending`/`record_error`), submitted sweep 조회 함수를 `src/services/gift/db.rs`에 이식. **수정:** `common::db::DbPool`/`with_retry` → api-server `sqlx::PgPool` 직접 사용 + 간단한 `with_retry` 헬퍼를 동일 파일에 포팅(transient 판정 로직 그대로). 주소 컬럼 VARCHAR↔Address 변환 로직 그대로.

- [ ] **Step 2: 컴파일**

Run: `cargo build`
Expected: 성공

- [ ] **Step 3: Commit** — `feat(gift): port consumer gift_tweet queries (claim/transition/sweep)`

### Task 3.2: validator.rs 이식 (preflight)

**Files:**
- Create: `src/services/gift/validator.rs`

- [ ] **Step 1: 이식**

`gift-bot/crates/consumer/src/validator.rs` + `common/src/pipeline/validator.rs`의 preflight 로직(getGiftInfo 기반 `ReceiverEqualsGiftVault`/`NotConfigured`/`Burned`/`HandleMismatch`/`AlreadyBound`)을 `src/services/gift/validator.rs`로 이식. **수정:** import 경로(`common::chain::*`→`crate::chain::*`, `crate::domain`→`crate::services::gift::domain`). pure `evaluate` 함수 + 테스트도 그대로.

- [ ] **Step 2: 단위 테스트**

Run: `cargo test --lib services::gift::validator`
Expected: PASS (evaluate 분기 테스트 — anvil 불필요)

- [ ] **Step 3: Commit** — `feat(gift): port on-chain preflight validator`

### Task 3.3: reconcile.rs 이식

**Files:**
- Create: `src/services/gift/reconcile.rs`

- [ ] **Step 1: 이식** — `gift-bot/crates/consumer/src/reconcile.rs` 복사, import 경로만 수정(`crate::rpc_chain`→`crate::chain::rpc_chain`, `crate::validator`→`crate::services::gift::validator`). `REVERTED_REASON`/`ReconcileOutcome`/`reconcile_labeled` 그대로.

- [ ] **Step 2: 빌드** — `cargo build` 성공.
- [ ] **Step 3: Commit** — `feat(gift): port on-chain reconciliation (getGiftInfo source of truth)`

### Task 3.4: executor.rs 이식

**Files:**
- Create: `src/services/gift/executor.rs`

- [ ] **Step 1: 이식** — `gift-bot/crates/consumer/src/executor.rs` 복사. import 경로 수정(`crate::db`→`crate::services::gift::db`, `crate::reconcile`→`crate::services::gift::reconcile`, `crate::rpc_chain`→`crate::chain::rpc_chain`, `crate::validator`→`crate::services::gift::validator`, `common::chain::bindings`→`crate::chain::bindings`, `common::db`→`crate::services::gift::db`). `ExecutorConfig`/`ExecutionOutcome`/send+receipt+reconcile 로직 그대로.

- [ ] **Step 2: anvil 라운드트립 테스트** — gift-bot executor 테스트가 anvil을 띄운다면 함께 이식.

Run: `cargo test --lib services::gift::executor -- --include-ignored`
Expected: PASS (anvil 기동)

- [ ] **Step 3: Commit** — `feat(gift): port setReceiver executor (submit+receipt+reconcile)`

### Task 3.5: poller.rs 이식

**Files:**
- Create: `src/services/gift/poller.rs`

- [ ] **Step 1: 이식** — `gift-bot/crates/consumer/src/poller.rs` 복사. import 경로 수정(위와 동일 매핑). `common::db::{DbPool,with_retry}`→api-server pool + 포팅된 `with_retry`. `PgListener`로 `LISTEN gift_tweet_new` + 폴링 + `claim_pending` + 결과 전이 로직 그대로. `NOTIFY_CHANNEL = "gift_tweet_new"` 유지.

- [ ] **Step 2: 빌드 + 가능한 단위 테스트** — `cargo build`. (실 LISTEN 통합은 Task 3.7.)
- [ ] **Step 3: Commit** — `feat(gift): port consumer poller (LISTEN + interval claim loop)`

### Task 3.6: worker.rs — advisory-lock 단일 리더

**Files:**
- Create: `src/services/gift/worker.rs`
- Modify: `src/config.rs` (lock key 상수)

- [ ] **Step 1: lock key 상수**

`src/config.rs`에 추가:
```rust
/// pg_advisory_lock key for the gift consumer single-leader election.
/// Arbitrary fixed 64-bit constant; only the gift worker uses it.
pub const GIFT_CONSUMER_LOCK_KEY: i64 = 0x6749_4654_434F_4E53u64 as i64; // "gIFTCONS"
```

- [ ] **Step 2: worker.rs 작성**

`src/services/gift/worker.rs`:
```rust
//! Single-leader gift consumer. Every api-server instance calls spawn(); only
//! the one that wins pg_advisory_lock runs the consumer loop. On leader death
//! the session-scoped lock releases and another instance acquires it on its
//! next attempt. Webhook ingestion runs on every instance regardless.
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::time::interval;
use tracing::{error, info, warn};

use crate::config::GIFT_CONSUMER_LOCK_KEY;
use crate::state::AppState;

/// Spawn the leader-election + consumer task. Non-blocking; returns immediately.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let pool = state.postgres.get_pool(); // primary/write pool
        let mut retry = interval(Duration::from_secs(15));
        loop {
            retry.tick().await;
            // 전용 커넥션을 hold 하면서 lock을 잡는다(세션 스코프). 커넥션이 살아있는
            // 동안만 리더. 커넥션이 끊기면 lock 자동 해제 → 다른 인스턴스 승계.
            match try_become_leader(pool).await {
                Ok(Some(conn)) => {
                    info!("gift consumer: acquired advisory lock — running as leader");
                    if let Err(e) = run_leader(&state, conn).await {
                        error!(error = %e, "gift consumer leader loop exited; will re-contend");
                    }
                }
                Ok(None) => { /* 다른 인스턴스가 리더 — 다음 tick에 재시도 */ }
                Err(e) => warn!(error = %e, "gift consumer lock attempt failed"),
            }
        }
    });
}

/// pg_try_advisory_lock on a dedicated connection. Returns the held connection
/// if acquired (lock lives as long as the connection), else None.
async fn try_become_leader(
    pool: &PgPool,
) -> Result<Option<sqlx::pool::PoolConnection<sqlx::Postgres>>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    let got: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(GIFT_CONSUMER_LOCK_KEY)
        .fetch_one(&mut *conn)
        .await?;
    if got { Ok(Some(conn)) } else { Ok(None) }
}

/// Leader work: boot reconciliation sweep, then the poller loop + reply worker.
/// Returns when the loop ends (shutdown/error); caller re-contends.
async fn run_leader(
    state: &AppState,
    _lock_conn: sqlx::pool::PoolConnection<sqlx::Postgres>,
) -> anyhow::Result<()> {
    // _lock_conn은 함수가 사는 동안 hold → 리더십 유지. drop되면 lock 해제.
    // 1) RpcChain 부팅 + 권한 검증(canCall). 실패 시 워커 비활성(에러 반환).
    // 2) 부팅 reconciliation sweep (status='submitted' 전수).
    // 3) Poller::run(...) + ReplyWorker::run(...)을 select!로 동시 구동.
    //    -- gift-bot consumer/src/main.rs 부팅 시퀀스를 그대로 이식.
    todo!("wire RpcChain boot + sweep + poller + reply (ported from consumer/main.rs)")
}
```

> **이식 노트:** `run_leader`의 본문은 `gift-bot/crates/consumer/src/main.rs`의 부팅 시퀀스(§7.5: signer→RpcChain connect→verify_contract_code→canCall→boot sweep→poller)를 1:1로 옮긴 것이다. `todo!()`를 그 시퀀스로 교체하며, 실패 분기(권한 없음 등)는 gift-bot과 동일하게 처리하되 "process bail" 대신 "에러 반환 후 재경합"으로 매핑(서버는 죽지 않음). reply 워커는 Phase 4에서 `select!`에 추가.

- [ ] **Step 3: main.rs에서 spawn**

`src/main.rs`의 기존 api-key sync `tokio::spawn` 블록 아래에 추가:
```rust
// Gift consumer (single-leader via pg_advisory_lock). Webhook ingest runs on
// all instances; only the lock winner runs the on-chain consumer loop.
api_server::services::gift::worker::spawn(app_state.clone());
```

- [ ] **Step 4: 통합 테스트 — 단일 리더 보장**

`tests/`에 통합 테스트: 동일 DB pool로 `try_become_leader`를 두 번 호출 시 두 번째는 `None`(첫 커넥션이 lock 보유 중). 첫 커넥션 drop 후 재시도 시 획득.

Run: `cargo test gift_leader`
Expected: PASS (DB 필요 — 테스트 DB 연결)

- [ ] **Step 5: Commit** — `feat(gift): single-leader consumer via pg_advisory_lock`

### Task 3.7: consumer E2E (anvil + 테스트 DB)

**Files:**
- Test: `tests/gift_consumer_e2e.rs`

- [ ] **Step 1: E2E 테스트 작성**

webhook INSERT → NOTIFY → poller claim → (anvil GiftVault) setReceiver → reconcile → `completed` 전이까지를 검증하는 E2E. gift-bot consumer의 통합 테스트가 있다면 그 시나리오를 이식.

Run: `cargo test --test gift_consumer_e2e -- --include-ignored`
Expected: PASS

- [ ] **Step 2: Commit** — `test(gift): consumer end-to-end (ingest→setReceiver→completed)`

---

## Phase 4: reply 워커 (OAuth 1.0a)

### Task 4.1: reply.rs 이식

**Files:**
- Create: `src/services/gift/reply.rs`

- [ ] **Step 1: 이식**

`gift-bot/crates/consumer/src/reply.rs` + `common/src/x/reply.rs`(OAuth 1.0a 서명 헬퍼)를 `src/services/gift/reply.rs`로 이식. **수정:** OAuth 자격은 `GiftConfig`에서(`oauth_consumer_key`/`_secret`/`access_token`/`_secret`). reply 상태머신(`reply_status` none/pending/sent/failed) + `POST /2/tweets` 본문 `🎁 Gift activated! https://nad.fun/profile/{receiver}?tab=gift` 그대로. `hmac`/`sha1`/`base64`/`percent-encoding` 사용(Phase 0에서 추가됨).

> ⚠️ **rand 버전차 (이 모듈의 유일한 비-import 수정):** gift-bot은 `rand = 0.9`, api-server는 `rand = 0.8.5`. OAuth nonce 생성부에서 0.9 API(`rand::rng()` 등)를 0.8 API(`rand::thread_rng()`, `Rng::gen`/`gen_range`)로 소폭 조정한다. nonce는 "랜덤 32 hex/영숫자 문자열"이면 충분하므로 동작 의미는 동일. 나머지 OAuth 서명 base string 구성은 1:1.

- [ ] **Step 2: OAuth1 서명 단위 테스트** — gift-bot의 reply 서명 테스트(known signature base string) 이식.

Run: `cargo test --lib services::gift::reply`
Expected: PASS

- [ ] **Step 3: Commit** — `feat(gift): port OAuth1 reply worker (reply state machine)`

### Task 4.2: worker.rs에 reply 워커 연결

**Files:**
- Modify: `src/services/gift/worker.rs`

- [ ] **Step 1: run_leader의 select!에 ReplyWorker::run 추가** — gift-bot consumer/main.rs에서 reply 워커를 poller와 함께 구동하는 방식 그대로. `GIFT_X_REPLY_ENABLED` 동등 토글이 gift-bot에 있다면 동일 동작 유지(없으면 항상 on).

- [ ] **Step 2: 빌드 + reply 통합** — `cargo test gift` 통과.
- [ ] **Step 3: Commit** — `feat(gift): run reply worker alongside poller under leader`

---

## Phase 5: 메트릭 / 헬스

### Task 5.1: gift 메트릭 카운터

**Files:**
- Modify: `src/metrics/` (기존 메트릭 레이어), `src/router/gift/handler.rs`, `src/services/gift/*`

- [ ] **Step 1: 카운터 추가**

설계 §9 메트릭을 api-server `src/metrics` 패턴에 맞춰 추가:
- `gift_webhook_events_total{kind}`, `gift_webhook_signature_failures_total`, `gift_webhook_crc_total`, `gift_webhook_ingested_total`
- consumer 게이지: submitted/completed/rejected 카운트(gift-bot common::metrics consumer 항목 이식)

핸들러/ingest/poller/executor의 해당 지점에서 증가.

- [ ] **Step 2: 테스트** — 메트릭 증가 단위 테스트. `cargo test gift_metrics` PASS.
- [ ] **Step 3: Commit** — `feat(gift): webhook + consumer metrics`

### Task 5.2: 최종 검증 + 문서

**Files:**
- Modify: `docs/backend/complete.md` (또는 해당 changelog), 설계 문서 상태 업데이트

- [ ] **Step 1: 전체 테스트**

Run: `cargo test` (그리고 `cargo test -- --include-ignored`로 anvil/DB 통합 포함)
Expected: 전부 PASS. `cargo clippy --all-targets` 경고 0 목표.

- [ ] **Step 2: DRY_RUN 부팅 스모크**

`GIFT_DRY_RUN=true`로 로컬 기동 → `/gift/healthz` 200, CRC curl 응답 확인, 가짜 서명 POST 401, 유효 서명 빈 payload 200.

- [ ] **Step 3: changelog/문서 업데이트 + Commit**

CLAUDE.md `chang.md`→`complete.md` 라이프사이클에 따라 항목 이동. 설계 문서 상태를 "구현 완료"로.
```bash
git add -A
git commit -m "docs(gift): mark webhook migration implemented + changelog"
```

---

## Self-Review (작성자 체크)

**Spec 커버리지:** 설계 §4 모듈 배치 → Phase 0~5 전부 매핑. §5 인입(CRC/서명/payload/ingest/미들웨어 우회) → Phase 2. §6 consumer(preflight/executor/reconcile/poller/rpc_chain/권한검증/advisory-lock) → Phase 3. reply → Phase 4. 메트릭/헬스 → Phase 5. env/deps → Phase 0·1. 컷오버는 운영 절차(설계 §8)라 코드 플랜 밖 — 의도적.

**미해결(구현 중 확정):**
- `PostgresDatabase`의 정확한 write-pool 접근자명(`get_pool`/`get_write_pool`/`get_primary_pool`) — Task 2.7 Step 1에서 `src/db/postgres` 확인 후 통일.
- `api_key_gate` 정확한 시그니처/`next` 사용 — Task 2.7 Step 5에서 기존 코드 확인 후 맞춤.
- gift-bot consumer/main.rs 부팅 시퀀스 세부 — Task 3.6에서 원본 대조하며 이식.
- ingest 테스트의 공유 GiftConfig 헬퍼 — Task 2.1/2.6에서 `pub(crate)`로 노출.

**타입 일관성:** `GiftConfig`(필드명 prefix 제거형), `GiftParser`(rename), `ParsedGift`, `GiftRuntime`가 Phase 전반에서 일관. db 주소 표기 lowercase `{:#x}`로 gift-bot과 동일.

> 위 "미해결"은 placeholder가 아니라 **원본 코드 대조가 필요한 이식 지점**이다. 각 Task가 해당 원본 파일 경로와 수정 규칙을 명시하므로 실행자는 결정적으로 처리할 수 있다.
