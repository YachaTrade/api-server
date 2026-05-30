# Gift Bot — X Webhook 통합 (api-server)

기존 `gift-bot` 워크스페이스(producer + consumer + reply)의 역할을 api-server로 이전한 기능. X 인입을 filtered stream에서 **Account Activity webhook**으로 교체하고, 나머지 로직(parser / preflight / `setReceiver` / reconcile / reply / `gift_tweet` 상태머신)은 gift-bot 코드를 **1:1로 이식**했다.

- 설계: [`docs/superpowers/specs/2026-05-29-gift-bot-webhook-migration-design.md`](../superpowers/specs/2026-05-29-gift-bot-webhook-migration-design.md)
- 구현 플랜: [`docs/superpowers/plans/2026-05-29-gift-webhook-migration.md`](../superpowers/plans/2026-05-29-gift-webhook-migration.md)

## 아키텍처

```
X (Account Activity API) ─ CRC GET / event POST ─▶ Cloudflare ─▶ haproxy ─▶ api-server
  [웹훅 수신: 전 인스턴스 stateless]
    GET  /x/webhook     → CRC HMAC-SHA256 응답
    POST /x/webhook     → 서명 검증(raw body) → GiftParser → V2 allowlist(token 테이블) → INSERT gift_tweet ON CONFLICT DO NOTHING
                                                          │ pg_notify('gift_tweet_new')
  [consumer 워커: pg_advisory_lock 단일 리더]              ▼
    LISTEN + 폴링 → preflight(getGiftInfo) → setReceiver → reconcile → (reply)
                                                          ▼  단일 봇 EOA (nonce 충돌 없음)
                                                       Monad chain
```

## 엔드포인트

| 메서드 | 경로 | 설명 |
|--------|------|------|
| `GET` | `/x/webhook?crc_token=…` | X CRC challenge. `{ "response_token": "sha256=base64(HMAC-SHA256(consumer_secret, crc_token))" }` |
| `POST` | `/x/webhook` | Account Activity 이벤트. `x-twitter-webhooks-signature` 검증(불일치 401) → `tweet_create_events` 파싱 → `gift_tweet` 멱등 INSERT. 항상 200(서명 통과 시) |
| `GET` | `/x/healthz` | haproxy 백엔드 liveness |

- 세 경로 모두 `api_key_gate` 미들웨어 **우회**(X는 `X-API-Key`를 보내지 않음, 인증은 HMAC 서명).
- `GIFT_*` env 미설정 시 gift 런타임 부재 → 웹훅 503 + 워커 미기동, **본 API는 정상 부팅**(graceful degradation).

## 단일 리더 (consumer)

api-server는 다중 리전·다중 인스턴스. consumer 폴링 루프는 단일 봇 EOA로 tx를 보내므로 `pg_advisory_lock(GIFT_CONSUMER_LOCK_KEY)`로 **한 인스턴스만** 실행한다. 전용 커넥션에 lock을 잡고 5초 heartbeat(`SELECT 1`)로 커넥션 상실을 감지해 리더십을 양도(재경합). 리더 교체 창의 중복은 `FOR UPDATE SKIP LOCKED` + preflight `AlreadyBound` + 온체인 reconcile로 흡수.

## 설정 (`GIFT_*` env)

**반드시 설정 (gift 전용, 기존 변수 없음):** `GIFT_X_OAUTH_*`(4-tuple, CRC/서명/reply 공용), `GIFT_BOT_PRIVATE_KEY`(vault OPERATOR_ROLE 핫월렛).

**기존 변수 재사용 (GIFT_ 오버라이드 미설정 시 자동 fallback):**

| GIFT_ 오버라이드 | fallback (기존 api-server 변수) |
|---|---|
| `GIFT_MAIN_RPC_URL` | `RPC_URL` |
| `GIFT_MONAD_CHAIN_ID` | `CHAIN_ID` |
| `GIFT_VAULT_ADDRESS` | `V2_GIFT_VAULT` |
| consumer `LISTEN` | `PRIMARY_DATABASE_URL` |

**기본값 있음 (선택):** 파서 슬롯(`GIFT_X_MENTION_ACCOUNT`=nadfunnews / `_ACTIVATION_PREFIX`=Activating Gift for / `_RECIPIENT_PREFIX`=Fees will go to / `_REQUIRED_HASHTAG`=#Nadfun), `GIFT_X_WEBHOOK_ID`(**optional** — 수신/consumer 경로 미사용, 등록 전 비워둬도 됨), `GIFT_SUB_RPC_URL_1/2`, `GIFT_DRY_RUN`/`GIFT_PAUSED`/`GIFT_X_REPLY_ENABLED`, 타임아웃·폴링(`GIFT_CONSUMER_POLL_INTERVAL_MS`/`GIFT_RPC_READ_TIMEOUT_MS`/`GIFT_RPC_SEND_TIMEOUT_MS`/`GIFT_TX_CONFIRMATIONS`/`GIFT_TX_GRACEFUL_DRAIN_MS`/`GIFT_CONSUMER_WAIT_TIME_MS`). 전체 목록은 `.env.example`의 `GIFT_*` 블록 참조.

`gift_tweet` 테이블은 공유 `Naddotfun/migrations` 서브모듈(`migrations/0020_gift_tweet.sql`)에 이미 존재 — 스키마 변경 없음.

## 운영 셋업 (운영자 수동 — 구현 범위 밖)

api-server는 CRC/이벤트 엔드포인트만 제공한다. X webhook 등록(`POST /2/account_activity/webhooks`)·`@nadfunnews` OAuth authorize·subscription 생성은 운영자가 X API로 직접 수행하고, 받은 webhook id를 `GIFT_X_WEBHOOK_ID`에 설정한다.

## 컷오버

`GIFT_DRY_RUN=true`로 배포 → 테스트 트윗 INSERT/reconcile 로그 확인 → `GIFT_DRY_RUN=false` → 기존 gift-bot producer/consumer 정지.

## 메트릭

`METRICS.gift` (커스텀 `AtomicU64` 카운터, monitor 주기 로깅): `webhook_events`, `webhook_signature_failures`, `webhook_crc`, `webhook_ingested`, `tx_success`, `tx_failure`, `reply_success`, `reply_failure`, `db_read_errors`.

## 알려진 한계

**브로드캐스트 후 `mark_submitted` 기록 사이의 크래시 창** (executor `submit`): `setReceiver` tx를 `send_setreceiver`로 브로드캐스트한 뒤 `mark_submitted`(tx_hash 기록, `pending→submitted`)를 커밋한다. 이 둘 사이 ms 순간에 프로세스가 죽고 + DB가 `with_retry` 3회 내내 다운이면, tx는 체인에 살아있는데 row는 `pending`(tx_hash 없음)으로 남아 boot reconciliation sweep(`submitted`만 스캔)이 추적하지 못한다.

- **안전성**: 자금 손실/중복 전송은 없다. 재시작 후 해당 행을 `pending`으로 재클레임 → preflight가 ① 이미 mined+bound면 `AlreadyBound`(tx 미전송), ② 멤풀 잔존이면 **결정적 nonce**로 재서명 → 동일 tx(`AlreadyKnown`, 멱등), ③ 드롭이면 정상 재전송. 최악은 회계상 `completed` 대신 `AlreadyBound`로 로깅되는 정도.
- **출처**: gift-bot 원본의 의도된(문서화된) 트레이드오프를 그대로 이식. 마이그레이션 회귀가 아님.
- **하드닝(후속)**: RpcChain의 sign/broadcast를 분리해 서명→tx_hash 계산→`mark_submitted`(DB) 먼저→브로드캐스트 순으로 바꾸면 크래시 시에도 `submitted`+tx_hash가 남아 boot sweep이 추적 가능. 송신 경로 변경이라 별도 검증 필요.

## 테스트 / 검증

- 유닛 테스트 141 통과 (parser 39, OAuth1/CRC/서명, payload, config, db_retry, validator/reconcile, executor classify 등).
- 온체인 라운드트립(anvil)은 alloy `node-bindings` ↔ redis `tempfile` 핀 충돌로 `#[cfg(feature = "node-bindings")]` 게이트 유지(기본 빌드 제외). 온체인 경로는 gift-bot 리포에서 anvil 검증 완료 + 스테이징 `DRY_RUN` 수동 검증으로 갈음.
