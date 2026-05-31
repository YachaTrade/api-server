# BalanceInfo 개편 — `lp_balance` + `quote_price` 추가 설계

> **개정 이력:** 초안 → codex consult 리뷰(2026-05-31, session 019e7ba5) 반영 개정.

**Goal:** `BalanceInfo`에 두 필드를 flat 추가한다.
1. `lp_balance` — 지갑 보유분(`balance`)과 별개로, LP에 잠긴 해당 토큰의 환산 수량. 토큰 `version`에 따라 소스가 다름 (V1 = Capricorn, V2 = 자체 `lp_position`).
2. `quote_price` — V2 dual-field 전략(legacy `native_*` → canonical `quote_*`)에 맞춰 `BalanceInfo`에도 누락된 `quote_price`를 채운다.

**Tech Stack:** Rust, Axum, sqlx, serde, utoipa, `reqwest`(Capricorn GraphQL client 신규).

**Scope:** `BalanceInfo`를 쓰는 6개 엔드포인트 전부. `quote_price`·`lp_balance` 모두 6곳에 채운다.

---

## 배경

### 현재 상태
- `BalanceInfo` (`src/types/common/info.rs:168`): `balance`(지갑), `token_price`(Token/USD), `native_price`(legacy), `created_at`.
- `balance`는 지갑 직접 보유분만. **LP에 잠긴 토큰 미반영.**
- `MarketInfo`(info.rs:112-114)·`SwapInfo`(info.rs:158-159)는 `native_price`+`quote_price` 쌍 보유. **`BalanceInfo`만 `quote_price` 누락.**
- V2 LP→토큰 환산 로직은 `dex/position.rs`에 존재하나 **`my_liquidity_usd`/`current_share`가 `pub(crate)`/private** — 그대로는 재사용 불가, helper 추출 필요 (codex 지적).

### 데이터 소스
| 항목 | 소스 |
|---|---|
| 지갑 `balance` | `balance` 테이블 |
| `quote_price` | `price` 테이블, `quote_id` LATERAL 최신 (= 현재 `native_price`와 동일 값) |
| V2 LP | 자체 `lp_position`(account_id,pool_id PK, `balance` GENERATED = lp_in-lp_out) + `pool`(reserve0/1, total_supply) |
| V1 LP | **Capricorn 외부 GraphQL** — prod endpoint(env). 자체 DB에 V1 LP 없음 |

---

## 1. Struct 변경

`src/types/common/info.rs:168`

```rust
/// Balance information with pricing (all prices in $)
pub struct BalanceInfo {
    /// Wallet-held token balance (balance 테이블)
    pub balance: String,
    /// LP-locked amount of this token, in the SAME token units as `balance`. "0" when none.
    /// V1: Capricorn position amountHuman 합산; V2: lp.balance × pool.reserveN / pool.total_supply
    pub lp_balance: String,        // NEW
    /// Token/USD price
    pub token_price: String,
    /// quote/USD price (legacy alias; 과거 "MON/USD"였으나 실제로는 quote_id 기준 USD)
    pub native_price: String,
    /// quote/USD price (canonical). 현재 native_price와 동일 값
    pub quote_price: String,       // NEW
    // holding period (지갑 balance 기준; lp_balance와 무관)
    pub created_at: i64,
}
```

- `native_price` 주석 정정: 기존 "MON/USD"는 거짓 (codex 지적). 실제값은 hold-token 등 모든 쿼리에서 `COALESCE(lp.price,0)` = `price` 테이블의 `quote_id` 기준 USD. `quote_price`도 동일 소스.

---

## 2. `quote_price` 채움 (6곳, 쿼리 무변경)

모든 대상 쿼리가 이미 `COALESCE(lp.price, 0) as native_price` (price 테이블 `quote_id` LATERAL)를 select. `MarketInfo`는 이미 `quote_price: row.native_price` 사용 (create.rs:301, gift_fee.rs:305).

→ **각 `BalanceInfo` 생성부에 `quote_price: row.native_price.normalized().to_plain_string()` 한 줄.** SQL 무변경. 현재 `quote_price == native_price`. FE 마이그레이션 후 `native_price` 제거.

---

## 3. `lp_balance` 채움 — version 분기, 2단계

### 흐름
```
① SQL 1차: 토큰 목록 + V2 lp_balance 계산 (V1 토큰은 SQL 단계에서 NULL → 후속 보강 대상)
② V1 토큰이 있으면 → Capricorn 배치 호출(엔드포인트당 1회 + 필요시 페이지네이션) → 정규화 매칭으로 보강
③ Capricorn 실패/타임아웃 → 해당 토큰 lp_balance="0" (에러 절대 전파 금지)
```

### V2 (자체 DB) — codex 정정 반영
- 토큰 pool: `market.pool_id` **하나만 조회** (사용자 결정 — multi-pool 합산 안 함).
- `lp_position` (account_id, pool_id) 1행, `balance` GENERATED.
- **reserve 선택 (codex: `reserve_token` 컬럼 없음):**
  ```sql
  CASE
    WHEN p.token0 = t.token_id THEN lp.balance * p.reserve0 / NULLIF(p.total_supply, 0)
    WHEN p.token1 = t.token_id THEN lp.balance * p.reserve1 / NULLIF(p.total_supply, 0)
    ELSE 0
  END AS lp_balance
  ```
  `token_id`와 `pool.token0/token1`은 동일 `VARCHAR(42)` 주소 공간 (`dex_token.token_id = pool.token0` 조인이 근거).
- `total_supply <= 0` → `NULLIF` + `COALESCE(..., 0)`로 `"0"`.
- `V2_CURVE`(미graduate, pool 없음) → `"0"`.
- **decimals:** `lp.balance`·`reserve`·`total_supply` 모두 raw `NUMERIC`. 공식상 LP 단위가 상쇄되고 reserve(토큰 raw) 단위가 남아 `balance`와 **동일 단위** → `balance`와 같은 `.normalized().to_plain_string()` 처리로 일관.
- **helper 추출:** `dex/position.rs`의 환산 로직을 `pub(crate) fn lp_token_amount(lp_balance, reserve, total_supply) -> BigDecimal`로 추출해 SQL 대신/병행 사용. (그대로는 private이라 재사용 불가 — codex 지적)

### V1 (Capricorn GraphQL)
- 자체 DB에 데이터 없음 → Capricorn `positions(where: {...})`.
- position의 `token0/token1` 중 `token_id`와 매칭되는 쪽 `amountHuman` 합산 (동일 owner가 여러 position이면 합산).

---

## 4. Capricorn client (신규 모듈)

`src/services/capricorn/` 신규. `reqwest` 기반.

### endpoint
- **prod URL을 env로** (`CAPRICORN_GRAPHQL_URL`). gist의 `dev-api.capricorn.exchange`는 **dev 전용 — 프로덕션 사용 금지** (codex 지적). `.env.example` 추가.

### schema (introspect 확인됨)
- `positions(where: PositionFilter, token0Addresses:[..], token1Addresses:[..], activeOnly, limit, offset)`
- `PositionFilter { owner: StringComparison, poolId: StringComparison, ... }` — `StringComparison` 연산자(eq/in) 정확한 형태는 구현 시 introspect (§9).
- position: `owner, token0{tokenAddress}, token1{tokenAddress}, amount0/1, amount0Human/amount1Human`.

### 인터페이스
```rust
async fn fetch_by_owner(&self, owner: &str) -> Vec<CapricornPosition>;  // account 기준 5곳
async fn fetch_by_token(&self, token_addr: &str) -> Vec<CapricornPosition>; // holder
```

### 🔴 주소 정규화 (codex 지적 + memory: EIP-55)
- 우리 `account_id`/`token_id`는 EIP-55 checksum, Capricorn은 lowercase.
- **매칭은 메모리 내 lowercase 일시 비교만** (owner, token0/1, account_id 전부 `to_lowercase()`). DB 저장·쿼리는 checksum 유지 (LOWER() 비교 금지 규칙 준수 — 외부 매칭에만 예외).

### 🔴 페이지네이션 (codex 지적)
- `fetch_by_token`은 high-LP 토큰에서 position이 많을 수 있음 → `limit/offset` 루프로 전량 수집, 부분 실패 시 그때까지 수집분만 사용 (silent 잘림 금지, 로그).

### 🔴 실패 처리 (사용자 강조 — 절대 에러 전파 금지)
- 실패 케이스 전부 → **빈 결과 → 해당 토큰 `lp_balance="0"`**, `tracing` 경고만, 응답 200 유지:
  network/timeout/HTTP 4xx5xx/`429`/GraphQL `errors`/JSON 파싱 실패/schema drift/부분 페이지 실패.
- `reqwest` `.timeout(...)` 명시 (짧게, 예: 3s).
- **캐싱 2단:** (1) 엔드포인트 응답은 `with_cache` **1초 TTL**(single_flight.rs) — 실패시 `"0"`이 최대 1초 노출, 수용 (사용자 결정). (2) **Capricorn raw 결과(`fetch_by_owner`/`fetch_by_token`)는 owner/token 키로 Redis 별도 캐시(30~60s TTL) + single_flight dedup** → holder 여러 페이지·동시 요청이 외부 호출 1회를 공유 (페이지마다 외부 반복 제거).

---

## 5. 엔드포인트별 호출 단위 (6곳 전부)

| 엔드포인트 | 기준 | Capricorn(V1만) |
|---|---|---|
| `GET /profile/hold-token/{account_id}` | account | `fetch_by_owner` 1회 |
| `GET /agent/holdings/{account_id}` | account | `fetch_by_owner` 1회 |
| `GET /profile/tokens/created/{account_id}` | account | `fetch_by_owner` 1회 |
| `GET /profile/gift-fee/{account_id}` | account | `fetch_by_owner` 1회 |
| `GET /agent/token/created/{account_id}` | account | `fetch_by_owner` 1회 |
| `GET /trade/holder/{token_id}` | token | `fetch_by_token` 1회(+페이지) |

**★ 방식: 6곳 전부 "기존 query + 보강".** LP-only(지갑 0 + LP만)는 **어느 목록에도 새 행으로 추가하지 않는다.** 기존 쿼리가 목록·정렬·페이지·`total_count`를 그대로 정하고, 각 행에 `lp_balance`/`quote_price`만 얹는다. → holder 합집합 재설계·`unnest` 주입·`total_count` 재계산 전부 불필요, max 9.7만 홀더 토큰도 안전.

- **V2 보강:** 기존 쿼리에 `LEFT JOIN lp_position`(해당 `account_id` + `market.pool_id`) → `lp.balance × (token0=token_id?reserve0:reserve1) / NULLIF(total_supply,0)`. SQL 한 방.
- **V1 보강:** 기존 쿼리로 페이지 행 확정 → Capricorn(account 5곳=`owner`, holder=`token` 1회, §4 캐시) → `account_id`(lowercase) 매칭 → `lp_balance`, 매칭 없으면 `"0"`.
- **알려진 한계 (의도적):** 지갑 0인데 LP에만 있는 토큰/홀더는 목록에 안 나타남 — 특히 `hold-token`에서 내가 LP에 전량 넣은 토큰은 누락. 추후 필요 시 union 확장 (§9).
- `holder`의 `total_count`·정렬은 **기존 그대로** 유지(변경 없음). created/gift는 `balance`가 보통 0이어도 `lp_balance`는 동일 account 기준 보강.

---

## 6. 컨트롤러 통합 (codex 지적 — 과소평가됐던 부분)

- created/gift 컨트롤러는 **캐시 클로저 안에서 `db`만으로 재생성**됨 (create.rs:157, gift_fee.rs:158). Capricorn client 주입하려면:
  1. `AppState`에 `CapricornClient`(reqwest client + prod URL) 추가.
  2. 각 controller 생성자/클로저가 client를 캡처하도록 수정.
  3. `lp_balance` 보강은 SQL 1차 결과 받은 뒤 async Capricorn 호출 → 매핑 (캐시 클로저 내부 또는 직후).
- agent `holdings`/`created`가 `trading/position.rs`/`token/create.rs` controller를 공유하는지 확인 → 공유면 자동 반영.

---

## 7. 테스트 전략 (🔴 TDD 절대규칙) — codex 반영해 구체화

- **`quote_price`**: 직렬화 + `quote_price == native_price` 동등성.
- **V2 `lp_balance` SQL** (test DB fixture):
  - `token0=token_id`→reserve0, `token1=token_id`→reserve1 각각.
  - `total_supply=0` → `"0"`. `V2_CURVE`(pool 없음) → `"0"`.
  - decimals: 출력이 `balance`와 동일 단위.
- **V1 Capricorn client**:
  - 정상 응답 파싱 + owner/token 매칭 + **lowercase 정규화 매칭** + 다중 position 합산 + 페이지네이션(2페이지 이상).
  - **실패 fallback (각각):** timeout / HTTP 500 / 429 / malformed JSON / GraphQL errors / 부분 페이지 실패 → `lp_balance="0"` + 에러 미전파 + 응답 200. (사용자 강조)
  - **캐시 실패 케이스:** 실패→"0"이 1초 후 갱신되는지.
- **holder (기존+보강)**: 페이지 홀더에 V2(`lp_position` JOIN)·V1(Capricorn 매칭) `lp_balance`가 정확히 붙는지; LP 없는 홀더는 `"0"`; `total_count`·정렬은 기존 유지(변경 없음); LP-only 홀더가 목록에 안 나옴(의도된 동작) 확인.
- 커버리지 80%+.

---

## 8. 영향 파일

- `src/types/common/info.rs` — `BalanceInfo` 필드 2개 + `native_price` 주석 정정.
- `src/controllers/trading/position.rs` — holder(:149)·hold-token(:344) + V2 lp SQL + V1 보강.
- `src/controllers/token/create.rs`(:325)·`gift_fee.rs`(:329) — created/gift + V2 lp SQL + V1 보강 + 캐시 클로저에 client 캡처.
- `src/controllers/dex/position.rs` — 환산 helper `pub(crate)` 추출.
- `src/services/capricorn/`(신규) — GraphQL client (정규화·페이지네이션·실패→빈결과).
- `src/state.rs` — `CapricornClient` DI + prod URL.
- `.env.example` — `CAPRICORN_GRAPHQL_URL`.
- agent handlers — controller 공유 확인.
- Swagger — utoipa 자동.

---

## 9. 미해결 (plan에서 확정 / 추후)

- **LP-only 미표시 (의도적 제외):** 지갑 0 + LP만 보유 시 6곳 모두 목록 누락 (특히 hold-token에서 LP 전량 토큰). 필요 시 hold-token/holder를 `balance ∪ lp` union으로 확장 — 별도 작업.
- ~~`StringComparison` 연산자~~ → introspect 확인됨: `owner: { _eq }` 단일, `{ _in: [..] }` 배열. (해소)
- Capricorn `positions`에 **정렬 인자 없음** → 서버 정렬 불가, `limit`/`offset`만. 정렬은 항상 우리 DB가 처리 (보강 방식이라 영향 없음).
- `CAPRICORN_GRAPHQL_URL` 프로덕션 실제 값 확보.
- V1 토큰이 Capricorn에 여러 quote 페어로 존재 시 합산 (전 페어 합산 가정).
- created/gift에서 `created_at=0` + `lp_balance>0` 표시 의미 — FE 처리 합의.
- **API 버전 분리** (`2026-04-11-api-url-versioning`): 신규 필수 필드 추가가 unversioned(V1 스키마) 경로 계약에 미치는 영향 — strict client 호환. 교차 확인.
- 기존 `2026-04-11-price-quote-aware.md` plan과 `quote_price` 정의 정합성.

---

## Revision 2 (2026-05-31) — `total_balance` + 합집합 정렬 (사용자 결정, 실측 후)

실측(`0x3500…` V1 토큰: LP holder 47명·position 55·잠긴 토큰 ~1,200만)으로 **LP holder가 소수**임이 확인되어, 무거운 건 지갑(max 9.7만)뿐이고 LP는 `unnest` 주입이 가벼움이 검증됨. 이에 따라 §5의 "보강만 / LP-only 제외 / 지갑 순"을 **다음으로 대체**:

- **`BalanceInfo` + `total_balance`** (= `balance` + `lp_balance`), 6곳 공통 노출.
- **6곳 전부 지갑 ∪ LP 합집합 (LP-only 포함)** + `total_balance` **DESC 정렬** + `total_count` 합집합 distinct 재계산.
  - V2: `balance FULL OUTER JOIN lp_position`(account + `market.pool_id`).
  - V1: Capricorn(account=`fetch_by_owner` / holder=`fetch_by_token`, 단일 `token0Addresses`) → `(owner, lp_amount)` `unnest` 주입 → `balance`와 `FULL OUTER JOIN`.
- **cap:** Capricorn `total`이 임계(예: 2000) 초과 시 해당 토큰은 보강-only fallback(합산 정렬 부정확 가능, 로그). 실측상 대부분 무관.
- ⚠️ **created/gift 모호:** 목록 기준이 creator/receiver라 "LP-only 합집합"은 의미 약함 — `total_balance` 필드+정렬만 적용, LP-only 행 추가는 holder/hold-token/holdings에 한정 검토 (재구현 시 확정).

**현재 구현(8커밋) 재활용:** struct(+`total_balance` 추가), `quote_price` 미러, `current_share`, `CapricornClient`(fetch/aggregate/fail-to-empty). **재작성:** 6곳 쿼리(합집합), `total_balance` 정렬, `total_count`, 핸들러 보강 흐름.
