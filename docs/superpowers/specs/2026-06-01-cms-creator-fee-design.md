# CMS Creator-Fee 엔드포인트 설계

- 작성일: 2026-06-01
- 브랜치: `feat/cms-creator-fee` (base: `v2`)
- 상태: 승인됨 (design approved)

## 목표

CMS(admin)에서 특정 토큰의 기간별 거래량/크리에이터 수수료 집계를 조회하는 읽기 전용 엔드포인트 1개를 추가한다.
기존 `nadfun=#` 콘솔에서 수동 실행하던 CTE 쿼리를 API로 노출한다.

## 엔드포인트

```
GET /cms/analytics/creator-fee
```

- 인증: `authenticate_user` 미들웨어 + 핸들러 내 `verify_admin()` (기존 모든 `/cms/*`와 동일)
- 태그: `CMS Analytics` (기존 그룹 재사용)

### Query params (`IntoParams`)

| 이름 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `token_id` | String | ✅ | 진입 직후 `valid_token_id()`로 EIP-55 체크섬 + 베니티 검증/정규화. 실패 시 400 |
| `from` | i64 | ✅ | unix seconds. `created_at >= from` |
| `to` | i64 | ❌ | unix seconds. `created_at < to`. 생략 시 상한 없음 |
| `session` | Cookie | (admin) | `verify_admin()` |

검증:
- `from < 0` → 400
- `to` 제공 시 `to <= from` → 400
- `token_id` invalid → 400

존재 확인(`valid_existing_token_id`)은 하지 않는다 — 미존재 토큰은 자연스럽게 합계 0으로 응답(분석 집계 성격상 404보다 0이 적절, Redis 의존 제거).

### Response (`Serialize + ToSchema`)

```rust
struct CreatorFeeResponse {
    quote_id:          String,  // market.quote_id (미상 시 native WMON)
    quote_symbol:      String,  // "MON" | "USDC" ... 금액 단위 표기
    decimals:          i32,     // quote_token.decimals (스케일 분모)
    total_volume:      String,  // SUM(swap.quote_amount) / 10^decimals
    pure_creator_fee:  String,  // v1: SUM(lp_collect_history.c_amount)/10^decimals | v2: "0"
    sell_fee:          String,  // v1: SUM(fee_distribute_history.creator_amount)/10^decimals | v2: "0"
    total_creator_fee: String,  // v1: pure + sell | v2: SUM(v2_creator_fee_distribution.amount)/10^decimals
}
```

값 포맷: `(raw / 10^decimals).normalized().to_plain_string()` (bigdecimal 0.4.x).
정밀도 손실 없음, 지수표기 없음, 불필요한 끝자리 0 제거.

**decimals 출처 (multi-quote 정합성):** 토큰당 `market` 1행(PK=token_id)이므로 quote는 단일.
`market m JOIN quote_token qt ON m.quote_id=qt.quote_id`로 `decimals` 조회.
`market`/`quote_token` row 없으면 native WMON(18 decimals) 기본값. `swap`엔 quote_id 컬럼이
없어(canonical 스키마) market 경유로 해결. 이로써 USDC(6) 등 non-18 quote도 정확.

### 버전 분기

`token.version`(`'V1'` | `'V2'`)으로 자동 분기. 분기 키가 token row에 있으므로 **존재 확인 필수**:
토큰 미존재 → 404 NotFound.

- **total_volume**: v1/v2 동일. `SUM(swap.quote_amount)` (swap에 V2_CURVE/V2_DEX 행 포함, 버전 무관).
- **creator fee**:
  - v1: `pure_creator_fee = SUM(lp_collect_history.c_amount)`, `sell_fee = SUM(fee_distribute_history.creator_amount)`, `total = pure + sell`.
  - v2: `total_creator_fee = SUM(v2_creator_fee_distribution.amount WHERE token=$1 AND event_type='DISTRIBUTE')`. `pure_creator_fee = sell_fee = "0"`.

## 데이터 흐름

```
handler (router/cms/analytics/handler.rs)
  ├─ verify_admin()
  ├─ valid_token_id(token_id)      → 400 on None
  ├─ validate from/to              → 400 on invalid
  └─ AnalyticsController::creator_fee_stats(token_id, from, to)
        ├─ token JOIN market JOIN quote_token: version + decimals + symbol + quote_id
        │     → token row 없으면 404 (None)
        ├─ total_volume:  swap 집계 (v1/v2 공통)
        ├─ version == 'V2' → v2_creator_fee_distribution 집계 (total만)
        │  else (V1)       → lp_collect_history + fee_distribute_history 집계 (pure/sell)
        └─ Rust에서 4개 값 /10^decimals 포맷 (v1 total = pure + sell)
  → AppJsonResult<CreatorFeeResponse>
```

### SQL — 공통 (total_volume)

```sql
SELECT COALESCE(SUM(quote_amount), 0) AS total_volume
FROM swap
WHERE token_id = $1 AND created_at >= $2 AND ($3::bigint IS NULL OR created_at < $3);
```

### SQL — v1 creator fee (CTE)

```sql
WITH
collect_agg AS (
  SELECT COALESCE(SUM(c_amount), 0) AS pure_creator_fee
  FROM lp_collect_history
  WHERE token_id = $1 AND created_at >= $2 AND ($3::bigint IS NULL OR created_at < $3)
),
distribute_agg AS (
  SELECT COALESCE(SUM(creator_amount), 0) AS sell_fee
  FROM fee_distribute_history
  WHERE token_id = $1 AND created_at >= $2 AND ($3::bigint IS NULL OR created_at < $3)
)
SELECT c.pure_creator_fee, d.sell_fee FROM collect_agg c, distribute_agg d;
```

### SQL — v2 creator fee

```sql
SELECT COALESCE(SUM(amount), 0) AS total_creator_fee
FROM v2_creator_fee_distribution
WHERE token = $1 AND event_type = 'DISTRIBUTE'
  AND created_at >= $2 AND ($3::bigint IS NULL OR created_at < $3);
```

bind: `$1 = token_id (String)`, `$2 = from (i64)`, `$3 = to (Option<i64>)`.
주: v2는 `token` 컬럼명(=token_id), `event_type='DISTRIBUTE'`만(=`CallbackFail` 제외). quote_id 필터는 하지 않음(전체 amount 합산) — 추후 MON-only 필요 시 quote_id 조건 추가.

## 변경 파일

| 파일 | 변경 |
|------|------|
| `src/types/cms/analytics.rs` | `CreatorFeeQuery`(IntoParams), `CreatorFeeResponse`(ToSchema) 추가 |
| `src/controllers/cms/analytics.rs` | `CreatorFeeStatsRow`(FromRow) + `creator_fee_stats()` 메서드 + wei→MON 포맷 헬퍼 추가 |
| `src/router/cms/analytics/path.rs` | `AnalyticsPath::CreatorFee` variant + `/cms/analytics/creator-fee` |
| `src/router/cms/analytics/mod.rs` | route 등록 (`get(handler::get_creator_fee)` + auth layer) |
| `src/router/cms/analytics/handler.rs` | `get_creator_fee` 핸들러 + `#[utoipa::path]` |
| `src/main.rs` | openapi `paths(...)`에 핸들러, `components(schemas(...))`에 `CreatorFeeResponse` 등록 |

## 테스트 (TDD, 컨트롤러 co-located `#[sqlx::test(migrations = "./migrations-test")]`)

`creator_fee_stats()` 컨트롤러 메서드 단위로 검증 (기존 repo 패턴과 동일, `make_controller(pool)` + 직접 seed).
모든 seed/query의 token 주소는 **체크섬 주소**로 일관 사용.

1. **v1 정상 합산** — token(version V1) + swap + lp_collect_history + fee_distribute_history → total_volume, pure, sell, total(=pure+sell) /10^18 검증
2. **v2 정상 합산** — token(version V2) + swap + v2_creator_fee_distribution(DISTRIBUTE 2건) → total_volume(swap), total_creator_fee=Σamount, pure="0", sell="0"
3. **v2 CallbackFail 제외** — DISTRIBUTE + CallbackFail 행 혼재 → CallbackFail은 합산에서 제외
4. **기간 필터(v1)** — `[from, to)` 밖의 행 제외 (경계: `created_at == to` 제외, `== from` 포함)
5. **to 생략** — 상한 없이 from 이후 전부 포함
6. **행 없음** — 모든 값 `"0"` (존재하는 토큰, 데이터만 없음)
7. **토큰 미존재** — `creator_fee_stats`가 NotFound 계열 에러 반환
8. **포맷** — 소수 wei(1.5 MON = 1500000000000000000)→`"1.5"`, 정수 MON 끝자리 0 제거
9. **quote decimals 적용** — USDC(6) quote 토큰 + market seed → `/10^6` 스케일(예: raw 1e6 → `"1"`), decimals/quote_symbol/quote_id 반영 (codex P2 대응)

핸들러 레벨 검증(400 invalid token / 401 non-admin)은 기존 analytics 핸들러도 컨트롤러 단위 테스트만 두므로 동일하게 컨트롤러 테스트에 집중. token_id/from/to 검증 로직은 핸들러 가드.

## 검증 명령

```bash
cargo test --lib cms::analytics      # 좁은 스코프
cargo test -- --test-threads=1       # 필요 시 (sqlx::test는 격리 DB라 보통 무관)
```

## 결정 로그

- 기간: `from`(필수) + `to`(선택, 배타적 상한). start-only/프리셋 대신 임의 구간 — CMS 정산 유연성.
- 숫자: String(BigDecimal 변환) — 기존 analytics 응답 컨벤션 일치, 정밀도 보존.
- token 검증: 핸들러 진입 시 `valid_token_id`로 **EIP-55 체크섬 정규화**(+베니티) 후 모든 SQL bind에 체크섬 주소 사용. canonical = 체크섬, `LOWER()` 비교 안 함. 임의 주소 필요 시 `valid_account_id`로 교체 가능.
- 버전 분기: `token.version`. 분기 키가 token row라 미존재 = 404. total_volume은 버전 무관 swap 집계.
- v2 creator fee: `v2_creator_fee_distribution`(컬럼 `token`/`amount`), `event_type='DISTRIBUTE'`만. pure/sell 구분 없음 → `"0"`.
- `total_creator_fee`(v1)는 SQL이 아닌 Rust에서 raw 합산 후 변환 — 반올림/스케일 일관성.
- **multi-quote 정합성 (codex P2 대응)**: 금액을 `10^18` 하드코딩이 아니라 토큰 quote의 `10^decimals`로 스케일. 토큰당 quote 단일(market PK=token_id)이라 quote별 분해 불필요. 응답에 `quote_id`/`quote_symbol`/`decimals` 포함해 단위 명시. USDC(6) 등 non-18 quote 정확 처리.
