# gift-bot → api-server 통합 (X 웹훅 전환) — 설계 문서

**상태**: 설계 완료, 구현 미시작
**작성**: 2026-05-29
**브랜치(예정)**: `feat/gift-webhook` (from `v2`)
**대체 대상**: `Naddotfun/gift-bot` 워크스페이스 전체 (producer + consumer + reply)

---

## 1. 문제 & 목표

기존 `gift-bot`은 별도 Rust 워크스페이스 2-바이너리(producer + consumer)로 운영된다.

- **producer**: X filtered stream(long-lived pull) → `Parser::parse` → `INSERT gift_tweet`
- **consumer**: `LISTEN gift_tweet_new` + 폴링 → preflight → `GiftVault.setReceiver` 온체인 → 성공 시 X 답글

filtered stream의 connection cycle / `429 TooManyConnections` lag로 트윗 손실이 발생한다 (gift-bot `docs/plans/2026-05-19-x-webhook-receiver-design.md` §문제). gift-bot 리포에는 이미 별도 `webhook-receiver` 바이너리 + `gift.nad.fun` + 전용 cert로 webhook 전환을 설계해 두었으나(Phase 1 스켈레톤만 존재), api-server가 **이미 public HTTPS Axum 서비스**이고 동일한 `gift_tweet` 테이블을 공유하므로 별도 바이너리/도메인/cert를 세우는 대신 **api-server가 직접 흡수**한다.

**목표**: gift-bot의 역할 전체(수신 + consumer + reply)를 api-server로 이관하고, gift-bot 워크스페이스를 은퇴시킨다.

### 1.1 불변 원칙 — 로직 1:1 이식

> **행위가 바뀌는 곳은 X 인입 트랜스포트 단 한 군데(filtered stream → webhook)뿐이다.**
> parser / validator / executor / reconcile / reply / `gift_tweet` 상태머신은 gift-bot 코드를
> **동작 그대로 이식**한다. 재설계·"개선"·리팩터링 금지 (CLAUDE.md Rule 3 · Rule 11).

```
[기존 producer]  X filtered stream(pull) ─┐
                                          ├─→ Parser → INSERT gift_tweet → consumer → setReceiver → reply
[변경 후]        X webhook(CRC+서명 push) ─┘   └ 이 체인 전체 동작 동일, 스트림 연결부만 webhook 핸들러로 교체
```

---

## 2. 확정된 결정사항

| 항목 | 결정 | 근거 |
|------|------|------|
| 범위 | producer + consumer + reply **전부** api-server로 이전, gift-bot 은퇴 | 사용자 확정 |
| 인입 전환 | filtered stream **즉시 컷오버** (병렬 운영 없음) | 사용자 확정 |
| 웹훅 소스 | **X Account Activity API** (CRC + `x-twitter-webhooks-signature` HMAC-SHA256) | 사용자 확정 |
| 인증 자격 | 기존 reply용 **OAuth 1.0a 4-tuple 재사용** | gift-bot 설계 §4 |
| consumer 실행 | **`pg_advisory_lock` 단일 리더** (전 인스턴스 부팅 시 시도, 1개만 워커 실행, 리더 사망 시 자동 승계) | 다중 리전·다중 인스턴스(아래 §3)에서 동일 봇 EOA nonce 충돌 방지 |
| 웹훅 수신 | **전 인스턴스 stateless** 수신, `INSERT … ON CONFLICT DO NOTHING` 멱등 | HTTP는 어느 인스턴스로 들어와도 안전 |
| 호스트/경로 | 기존 api 호스트 + `POST/GET /gift/webhook` | 별도 도메인/cert 불필요 |

### 다중 인스턴스 전제 (consumer 싱글톤이 필요한 이유)

api-server는 다중 리전·다중 인스턴스로 운영된다 (haproxy `be_api`: `api-frgr-`×3, `api-sg-`×3, `api-ca-`×3 + AWS ohio/paris/singapore). **웹훅 수신**은 stateless라 어디로 라우팅되든 무방하지만, **consumer 워커는 인바운드 HTTP와 무관한 백그라운드 루프**라 프로세스마다 각자 돈다. 모든 인스턴스에서 루프가 돌면 동일 봇 EOA에서 수십 개 워커가 `setReceiver`를 동시 전송 → nonce 충돌·중복 전송. 따라서 `pg_advisory_lock`으로 **글로벌 단일 리더**만 워커를 실행한다 (공유 primary DB 사용, 인프라 추가 0).

---

## 3. 아키텍처

```
                 X (Account Activity API)
                        │ CRC GET / event POST
                        ▼
            Cloudflare → haproxy → api-server (모든 인스턴스)
                        │
   ┌────────────────────┴──────────────────────────────┐
   │  [웹훅 수신: 전 인스턴스, stateless]                  │
   │  GET  /gift/webhook → CRC HMAC-SHA256 응답          │
   │  POST /gift/webhook                                 │
   │   ├─ x-twitter-webhooks-signature 검증 (raw body)   │
   │   ├─ GiftParser::parse        (gift-bot 그대로)      │
   │   └─ INSERT gift_tweet ON CONFLICT DO NOTHING       │
   └────────────────────┬──────────────────────────────┘
                        │ trigger → pg_notify('gift_tweet_new')
                        ▼
   ┌─────────────────────────────────────────────────────┐
   │  [consumer 워커: pg_advisory_lock 단일 리더]           │
   │  부팅 시 try_advisory_lock → 획득한 1 인스턴스만 실행    │
   │   ├─ 부팅 reconciliation sweep (status='submitted')   │
   │   ├─ LISTEN gift_tweet_new + 주기 폴링                 │
   │   ├─ FOR UPDATE SKIP LOCKED 로 pending 1행 클레임       │
   │   ├─ preflight (getGiftInfo)   (gift-bot 그대로)       │
   │   ├─ setReceiver 전송 + receipt + reconcile           │
   │   └─ reply 워커 (OAuth1 POST /2/tweets)               │
   └────────────────────┬──────────────────────────────┘
                        ▼  단일 EOA → nonce 충돌 없음
                     Monad chain
```

---

## 4. api-server 모듈 배치

기존 컨벤션(`router/<domain>/{path,handler}.rs`, `services/<domain>/`)을 따른다.

```
src/
├── router/gift/                신규
│   ├── path.rs                 GiftPath enum (CRC, Event, Healthz)
│   └── handler.rs              GET/POST /gift/webhook, GET /gift/healthz
├── services/gift/              신규 (gift-bot 로직 1:1 이식)
│   ├── mod.rs
│   ├── config.rs               GiftConfig — RPC/봇키/볼트/OAuth/파서슬롯/webhook_id 로드+검증
│   ├── parser.rs               ← common::x::parser            (regex 3종, 동작 동일)
│   ├── crc.rs                  CRC challenge HMAC-SHA256(consumer_secret, crc_token)
│   ├── signature.rs            x-twitter-webhooks-signature 검증 (raw body HMAC)
│   ├── ingest.rs               payload → tweet_create_events 순회 → parse → INSERT
│   ├── domain.rs               ← common::domain (ParsedGift 등)
│   ├── db.rs                   ← consumer::db (gift_tweet 쿼리/상태전이)
│   ├── validator.rs            ← consumer::validator + common::pipeline::validator (preflight)
│   ├── executor.rs             ← consumer::executor (setReceiver + receipt)
│   ├── reconcile.rs            ← consumer::reconcile (getGiftInfo = source of truth)
│   ├── poller.rs               ← consumer::poller (LISTEN + 폴링 + 클레임)
│   ├── reply.rs                ← consumer::reply + common::x::reply (OAuth1)
│   └── worker.rs               advisory-lock 리더 선출 + poller/reply spawn 진입점
├── chain/                      신규 (← common::chain)
│   ├── mod.rs
│   ├── bindings.rs             sol! { GiftVault, ProtocolManager } (그대로)
│   ├── signer.rs               PrivateKeySigner 로드 (Debug 마스킹)
│   └── rpc_chain.rs            멀티 RPC failover [main, sub1?, sub2?] + chain_id 부팅 검증
```

### 진입점 변경 (`src/main.rs`)

- 라우터 머지에 `.merge(gift::router())` 추가.
- 기존 `tokio::spawn`(api-key usage sync, main.rs:446) 패턴 그대로 `gift::worker::spawn(app_state.clone())` 추가.
  - 워커는 부팅 시 `pg_try_advisory_lock(<상수 key>)` 시도 → 실패 시 주기적으로 재시도하며 webhook만 받는 대기 상태. 획득 시 부팅 sweep → poller/reply 실행.

### 상태 (`src/state.rs`)

`AppState`에 `gift: Arc<GiftRuntime>` 추가 — `GiftConfig`(파서 슬롯·OAuth·webhook_id), 선택적 `RpcChain`(부팅 검증 통과 시), 메트릭 핸들 보유. 웹훅 핸들러는 parser+OAuth 자격만 사용하고, 워커는 RpcChain까지 사용.

---

## 5. 인입 트랜스포트 (유일한 신규 행위)

### 5.1 CRC challenge — `GET /gift/webhook?crc_token=…`
```
response_token = "sha256=" + base64(HMAC-SHA256(consumer_secret, crc_token))
→ { "response_token": response_token }   (200)
```
등록 시 1회 + X의 주기적 재검증에 응답. api-server는 상시 가동이라 별도 바이너리 대비 유리.

### 5.2 이벤트 — `POST /gift/webhook`
1. `axum::body::Bytes`로 **원문 바이트** 수신 (JSON extractor가 소비하기 전).
2. `x-twitter-webhooks-signature: sha256=<base64>` == `HMAC-SHA256(consumer_secret, raw_body)` 검증. 불일치 → **401 즉시**.
3. raw_body deserialize → `tweet_create_events[]` 순회.
4. 각 tweet `text`/`screen_name`/`id_str` → `GiftParser::parse` (**gift-bot 로직 동일**).
5. `ParsedGift` → `INSERT INTO gift_tweet (…) ON CONFLICT (tweet_id) DO NOTHING` (producer와 동일 INSERT). 트리거가 `pg_notify('gift_tweet_new')` 발송.
6. **200 즉시** 응답 (X는 5초 내 응답 못 받으면 retry; 핸들러는 검증+INSERT만, 체인 tx 없음).

### 5.3 미들웨어 우회
`/gift/webhook`은 X-API-Key를 보내지 않으므로 `api_server::middleware::api_key_gate`에서 **경로 prefix 예외 처리**. 인증은 HMAC 서명이 담당한다. (참고: `method_based_timeout`의 upload 예외 목록과 동일한 prefix 분기 패턴 사용.)

---

## 6. consumer (로직 동일, 위치만 이동)

`gift-bot/crates/consumer` + `crates/common/{chain,pipeline,x,domain,db}`를 `src/services/gift/` + `src/chain/`으로 **동작 보존 이식**. 핵심 행위 — 변경 없음:

- **preflight** (`validator`): `getGiftInfo` 기반 reject 단계 — `ReceiverEqualsGiftVault` / `NotConfigured` / `Burned` / `HandleMismatch`, `gift.receiver == parsed.receiver` 시 `AlreadyBound`(무전송 성공).
- **executor**: `setReceiver(token, receiver)` 전송 → receipt → reconcile. `submitted` + tx_hash를 receipt 대기 전에 durable하게 기록.
- **reconcile**: `getGiftInfo` = source of truth. 런타임(전송 후) + 부팅(`submitted` 전수 sweep) 두 곳에서 사용.
- **poller**: `LISTEN gift_tweet_new` + `CONSUMER_POLL_INTERVAL_MS` 폴링, `FOR UPDATE SKIP LOCKED` 단건 클레임, 결과를 `completed`/`rejected`/`pending`/`submitted` 전이로 기록.
- **rpc_chain**: `[main, sub1?, sub2?]` failover, 모든 엔드포인트 `eth_chainId == MONAD_CHAIN_ID` 부팅 하드체크(불일치 시 terminal), 동일 서명 raw tx 재제출(idempotent, `AlreadyKnown`=성공).
- **부팅 권한 검증**: `GiftVault.authority()` → `ProtocolManager.canCall(bot, vault, setReceiverSelector)`. 권한 없음/지연 있음 → **워커만 비활성**(서버는 정상 기동, 웹훅 수신은 계속).
- **reply**: setReceiver 성공 후 `reply_status` 상태머신(`none`/`pending`/`sent`/`failed`)에 따라 OAuth1 `POST /2/tweets`로 `🎁 Gift activated! https://nad.fun/profile/{receiver}?tab=gift` 답글.

### advisory-lock 리더 (신규 래퍼 — `worker.rs`)
- 부팅 시 `pg_try_advisory_lock(GIFT_CONSUMER_LOCK_KEY)`.
- 획득 실패: 워커 미실행, N초마다 재시도(대기 인스턴스). 웹훅 수신은 계속.
- 획득 성공: 부팅 reconciliation sweep → poller + reply 워커 실행. lock은 세션 유지(프로세스/커넥션 종료 시 자동 해제 → 다른 인스턴스 승계).
- 셧다운: 기존 consumer 셧다운 의미(`TX_GRACEFUL_DRAIN_MS`) 보존.

---

## 7. 의존성 / env

### Cargo.toml 추가
- `hmac`, `sha1`, `percent-encoding` (OAuth 1.0a + CRC/서명 HMAC)
- 이미 보유: `alloy = "1.0.24"`, `sha2`, `base64`, `hex`, `regex`, `rand`

### env (`GIFT_*` 블록, `.env.example`에 주석과 함께 추가)
```
# 체인 (consumer)
GIFT_MAIN_RPC_URL=...                  # 필수
GIFT_SUB_RPC_URL_1=...  GIFT_SUB_RPC_URL_2=...   # 선택, 부팅 chain_id 검증
GIFT_MONAD_CHAIN_ID=...
GIFT_VAULT_ADDRESS=0x...
GIFT_BOT_PRIVATE_KEY=...               # 핫월렛 — 로그 마스킹, env 전용
GIFT_CONSUMER_POLL_INTERVAL_MS=...
GIFT_TX_GRACEFUL_DRAIN_MS=...
GIFT_DRY_RUN=true|false   GIFT_PAUSED=true|false

# X webhook + OAuth (수신·등록·reply 공용)
GIFT_X_OAUTH_CONSUMER_KEY=...  GIFT_X_OAUTH_CONSUMER_SECRET=...   # 서명/CRC 키
GIFT_X_OAUTH_ACCESS_TOKEN=...  GIFT_X_OAUTH_ACCESS_TOKEN_SECRET=...
GIFT_X_WEBHOOK_ID=...                  # 등록 후 박아둠

# 파서 슬롯 (producer와 동일 값)
GIFT_X_MENTION_ACCOUNT=nadfunnews
GIFT_X_ACTIVATION_PREFIX=Activating Gift for
GIFT_X_RECIPIENT_PREFIX=Fees will go to
GIFT_X_REQUIRED_HASHTAG=#Nadfun
```
> env prefix `GIFT_`는 api-server 기존 변수와의 충돌 방지를 위함. 값 자체는 gift-bot `.env`와 동일.

`gift_tweet` 테이블: 공유 `Naddotfun/migrations` 서브모듈에 이미 존재(`migrations/0020_gift_tweet.sql`, api-server 포함 확인) — **스키마 변경 없음**.

---

## 8. 컷오버 (즉시 전환)

1. api-server에 webhook + consumer 배포, **`GIFT_DRY_RUN=true`** 로 무전송 검증 (체인 읽기·reconcile만, tx 미전송).
2. haproxy: 기존 api 호스트로 `/gift/webhook` 라우팅 확인 (별도 backend/cert 불필요).
3. **[운영자 수동 — 구현 범위 밖]** X webhook 등록(`POST /2/account_activity/webhooks`, url = `https://<api-host>/gift/webhook`) → 받은 id를 `GIFT_X_WEBHOOK_ID`에 설정. `@nadfunnews` OAuth authorize → subscription 생성. **api-server는 등록/subscription 도구를 제공하지 않는다** — CRC/이벤트 엔드포인트만 노출하고, 등록은 운영자가 직접 수행.
4. 테스트 트윗 → `gift_tweet` INSERT 확인 → preflight/reconcile 로그 확인.
5. **`GIFT_DRY_RUN=false`** 로 전환 (봇 EOA가 vault operator-role 보유 + 가스 충분 전제).
6. **gift-bot producer + consumer 정지** → 리포 deprecate.

> **구현 범위**: api-server 코드(웹훅 수신 엔드포인트 + consumer 워커 + reply)만. X webhook 등록/subscription 생성은 운영자가 X API로 직접 처리한다. 등록용 스크립트/관리 엔드포인트는 만들지 않는다.

---

## 9. 리스크 / 미해결

- **X 5초 SLA**: 웹훅 핸들러는 서명검증 + INSERT만(체인 tx 없음)이라 빠름. 배포 후 평균 응답시간 측정.
- **리더 승계 지연**: 리더 사망 시 다음 폴링/재시도 주기만큼(수 초) 처리 공백. 허용 가능.
- **OAuth authorize 운영**: `@nadfunnews` 소유자의 1회 수동 authorize 필요 (운영팀 협의).
- **X webhook 단가**: Account Activity Pay-Per-Use 과금 여부 확인 필요 (gift-bot 설계 §11에서도 미해결).
- **봇키 노출면 확대**: advisory-lock 모델 특성상 키가 전 인스턴스 env에 존재 (사용자 승인된 트레이드오프). 로그 마스킹·env 전용·operator-role 스코프로 완화.
- **migrations 서브모듈 브랜치**: api-server는 `migrations`를 `heads/v2`로 추적 중이며 `0020_gift_tweet.sql` 포함 확인 — 추가 마이그레이션 불필요.

---

## 10. 단계 분해 (TDD — 각 Phase `cargo test` 통과 후 진행)

| Phase | 내용 | 테스트 초점 |
|-------|------|-------------|
| 1 | `src/chain/`(bindings/signer/rpc_chain) + `GiftConfig` 로드·검증 + 부팅 권한 체크 | signer 벡터, chain_id 불일치 terminal, config 검증 |
| 2 | webhook 수신: `crc` + `signature` + `parser`(이식) + `ingest` INSERT + 미들웨어 우회 | CRC 응답 정확성, 서명 위조 401, parser 동등성(gift-bot 케이스 재사용), ON CONFLICT 멱등 |
| 3 | consumer: `worker`(advisory-lock) + `poller` + `validator` + `executor` + `reconcile` + 부팅 sweep | preflight 분기, reconcile source-of-truth, 리더 단일성, anvil 라운드트립 |
| 4 | `reply` 워커 (OAuth1) | reply 상태머신 전이, OAuth1 서명 |
| 5 | 메트릭/헬스 노출 (webhook 이벤트/서명실패/CRC/ingest 카운터, consumer 게이지) | 메트릭 카운터, /gift/healthz |

---

## 11. 이식 출처 매핑 (1:1 추적용)

| api-server (대상) | gift-bot (출처) | 행위 변경 |
|---|---|---|
| `services/gift/parser.rs` | `common/src/x/parser.rs` | 없음 |
| `services/gift/ingest.rs` | `producer/src/ingest.rs` (stream 부분 제외) | 입력원: stream → webhook payload |
| `services/gift/crc.rs`, `signature.rs` | (신규 — gift-bot 설계 §6, `webhook-receiver` 미구현분) | 신규 |
| `services/gift/db.rs` | `consumer/src/db.rs` | 없음 |
| `services/gift/validator.rs` | `consumer/src/validator.rs` + `common/pipeline/validator.rs` | 없음 |
| `services/gift/executor.rs` | `consumer/src/executor.rs` | 없음 |
| `services/gift/reconcile.rs` | `consumer/src/reconcile.rs` | 없음 |
| `services/gift/poller.rs` | `consumer/src/poller.rs` | 없음 |
| `services/gift/reply.rs` | `consumer/src/reply.rs` + `common/x/reply.rs` | 없음 |
| `services/gift/worker.rs` | `consumer/src/main.rs` 부팅 시퀀스 | 단일 프로세스 → advisory-lock 리더 래핑 |
| `chain/bindings.rs`,`signer.rs`,`rpc_chain.rs` | `common/src/chain/*` | 없음 |
```
