# Terminal API V2 라우팅 설계 (bonding curve + DEX)

- **날짜**: 2026-06-03
- **브랜치**: v2
- **상태**: 설계 확정 (구현 대기)
- **관련 코드**: `src/router/terminal/`, `src/services/terminal/mod.rs`, `src/controllers/terminal/mod.rs`, `src/types/terminal/mod.rs`
- **외부 문서**: `docs/terminal-api.md` (갱신 필요), Notion "BE Terminal Api intergration"

---

## 1. 개요 / 목표

Terminal API는 GeckoTerminal 호환 외부 가격 집계용 엔드포인트다(루트 경로 5종). 현재 V1(단일 native quote = `WMON`) 가정으로 작성되어 V2(멀티 quote bonding curve + NadSwap DEX)를 정확히 표현하지 못한다.

**목표**: 데이터 소스 테이블을 바꾸지 않고, 모든 `WMON` 하드코딩을 `market.quote_id` 기준으로 교체하여 V2_CURVE / V2_DEX를 정확히 서빙한다. V1 응답은 100% 불변(비파괴적).

## 2. 범위

- **포함**: nadfun 라이프사이클 토큰만 (V2_CURVE → 졸업 → V2_DEX). token : quote = 1 : 1. 토큰 18 decimals, quote(WMON/LVMON) 18 decimals 고정.
- **제외**: 외부 DEX 상장 토큰(`dex_token` + `pool` 유니버스, 임의 token0/token1 페어, 비-18 decimals). 향후 별도 설계.

## 3. 현재 상태 & 갭 (코드 근거)

| # | 갭 | 위치 |
|---|---|---|
| 1 | quote 토큰이 `WMON` 하드코딩 (asset 정렬·reserve 배정) — `market.quote_id` 미사용 | `services/terminal/mod.rs:124,228,320,381` |
| 2 | `dexKey`가 V2_DEX → `"capricorn"` (Uniswap v3 의미와 충돌) | `mod.rs:132` |
| 3 | `feeBps` = `100` 하드코딩 (V2 가변 수수료 미반영) | `mod.rs:149` |
| 4 | swap `pairId`가 항상 현재 `pool_id` (졸업 토큰의 커브시절 스왑 오매핑) | `mod.rs:296` |
| 5 | `terminal-api.md`가 stale (`/terminal/*` prefix, `nadpump`, feeBps 30 — 코드와 불일치) | `docs/terminal-api.md` |

실제 라우트는 **루트 경로**(`/latest-block`, `/asset`, `/pair`, `/events`, `/:token_address`) — Notion이 현행, `terminal-api.md`가 stale.

## 4. 핵심 결정사항

### 4.1 이벤트 데이터 소스 (옵저버 코드 확인 완료)

졸업한 nadfun V2 토큰의:
- **DEX 스왑** → 통합 `swap` 테이블 (`market_type='V2_DEX'`, token-centric). `observer/src/event/v2/dex/receive.rs:752-848`
- **DEX join/exit (LP add/remove)** → 통합 `mint`/`burn` 테이블 (token-centric). `observer/.../receive.rs:900-934`
- **V2_CURVE는 mint/burn 없음** (수학적 커브)
- 졸업 초기 유동성 시딩은 `dex_mint`(pool-centric)에만 적재 → `/events` join에 안 잡힘 (한계)

→ 결론: terminal이 이미 읽는 `swap`/`mint`/`burn` 그대로 사용. **각 쿼리에 `quote_id`만 추가**. `dex_swap`/`dex_sync`/`dex_mint`/`dex_burn`(pool-centric)은 api-server 어디서도 안 읽으며 사용하지 않음.

### 4.2 dexKey 매핑

| market_type | dexKey |
|---|---|
| `CURVE` (V1) | `nadfun` |
| `DEX` (V1) | `capricorn` |
| `V2_CURVE` | `nadfun-v2` |
| `V2_DEX` | `nadswap` |

V1 커브(`nadfun`)와 V2 커브(`nadfun-v2`)를 dexKey로 구분, V2 덱스는 NadSwap 페어이므로 `nadswap`.

### 4.3 feeBps 산식 (컨트랙트 검증 완료)

| market_type | feeBps | 근거 |
|---|---|---|
| `CURVE`/`DEX` (V1) | `100` 고정 | 기존 유지 |
| `V2_CURVE` | `creator_fee_rate + curve_protocol_fee_rate` | `BondingCurve._getTotalFeeRate` (LP 없음, 스나이핑 패널티는 일시적이라 제외) |
| `V2_DEX` | `25 + creator_fee_rate + dex_protocol_fee_rate` | `NadFunPair.LP_FEE_RATE(25) + (creatorFeeRate + dexProtocolFeeRate)` |

- `NadFunPair.sol:214` → `totalFeeRate = LP_FEE_RATE + (config.creatorFeeRate + config.dexProtocolFeeRate)`, `LP_FEE_RATE = 25`(0.25%, `constant`).
- api-server에 `NADSWAP_LP_FEE_BPS = 25` 상수 정의(컨트랙트 출처 주석).
- `fee_config` 행 없음(인덱서 Setup 미수신) → V2 토큰의 `feeBps` **생략(None)** (틀린 0%/부분값 노출 방지).

## 5. API 계약 변경

### `GET /pair`

| 필드 | 변경 후 |
|---|---|
| `asset0Id`/`asset1Id` | `quote_id` vs token 정렬 (소문자 비교) |
| `dexKey` | 4.2 표 |
| `id` (pairId) | CURVE→`V1_BONDING_CURVE`, V2_CURVE→`V2_BONDING_CURVE`, DEX/V2_DEX→`pool_id` (유지) |
| `feeBps` | 4.3 표 |

### `GET /events`

| 항목 | 변경 후 |
|---|---|
| swap `asset0/1`, `reserves`, `priceNative` | `quote_id` 기준 정렬 |
| swap `pairId` | **이벤트별 `s.market_type`** 매핑 (CURVE→V1_BC, V2_CURVE→V2_BC, DEX/V2_DEX→pool_id) |
| join/exit (`mint`/`burn`) | `quote_id` 기준 정렬 (market JOIN). `pairId`=`market_id` 유지 |

### `GET /asset`, `GET /latest-block`, `GET /:token_address`
변경 없음.

## 6. 코드 변경 (파일별)

### A. `src/controllers/terminal/mod.rs`

**`PairRow`** — `get_pair` SQL에 `m.quote_id` + `fee_config` LEFT JOIN 추가:
```sql
SELECT t.token_id, t.created_at, t.transaction_hash,
       m.pool_id, m.market_type, m.quote_id, t.creator,
       fc.creator_fee_rate, fc.curve_protocol_fee_rate, fc.dex_protocol_fee_rate
FROM token t
JOIN market m ON t.token_id = m.token_id
LEFT JOIN fee_config fc ON fc.token_id = t.token_id
WHERE t.token_id = $1
```
구조체: `quote_id: String`, `creator_fee_rate/curve_protocol_fee_rate/dex_protocol_fee_rate: Option<i16>` 추가.

**`SwapEventRow`** — swap 쿼리에 `s.market_type`, `m.quote_id` 추가. 구조체에 `quote_id: String`, `market_type: String` 추가.

**`MintEventRow`/`BurnEventRow`** — `mint`/`burn` 쿼리에 `JOIN market m ON m.token_id = <tbl>.token_id` 추가하여 `m.quote_id` select. 구조체에 `quote_id: String` 추가.

### B. `src/services/terminal/mod.rs`

헬퍼 추가:
- `order_assets(token_id, quote_id) -> (asset0, asset1, is_quote_token0)` — `WMON` 대신 `quote_id` 소문자 비교.
- `pair_id_for(market_type, pool_id) -> String`
- `dex_key_for(market_type) -> &'static str`
- `fee_bps_for(market_type, creator, curve, dex) -> Option<u32>`

적용:
- `get_pair`: 위 헬퍼로 교체.
- `convert_swap_event`: `is_native_token0` → `is_quote_token0`(quote_id 기준), `pair_id = pair_id_for(row.market_type, row.pool_id)`.
- `convert_mint_event`/`convert_burn_event`: quote_id 기준 정렬. `pair_id = market_id` 유지.
- 미사용 시 `WMON` import 제거.

### C. 변경 없음
`get_latest_block`, `get_asset`, `/:token_address`, `types/terminal/mod.rs`, 라우터.

## 7. 엣지케이스 / 정합성

1. `market.quote_id`는 `NOT NULL DEFAULT WMON` → JOIN 항상 안전.
2. decimals: nadfun 토큰(18) + quote(18)이라 `/10^18` 유지. 외부 토큰 제외로 안전.
3. 졸업 토큰 커브시절 swap → per-event `s.market_type`로 pairId 정확 분기 (기존 버그 수정).
4. `priceNative` = quote 기준 가격(LVMON quote면 LVMON 단위). 문서에 명시.
5. 출력 주소는 DB 저장값(EIP-55 체크섬) 그대로. 정렬은 소문자 비교라 무관.
6. 하위호환: V1 토큰(quote_id=WMON) 응답 불변.

## 8. 알려진 한계 (문서 명시)

- 졸업 초기 유동성 시딩(curve→pool)은 `dex_mint`에만 적재 → `/events` join 미노출. 사용자 LP add/remove(`mint`/`burn`)만 노출.
- `/pair`는 토큰의 **현재** 페어만 반환. 졸업 토큰 과거 커브 페어는 `/events`의 per-event pairId로만 추적.

## 9. 테스트 계획 (TDD, 현재 terminal 테스트 0개)

- 헬퍼 단위테스트: `order_assets`(quote=WMON/LVMON 양방향), `dex_key_for`(4종), `pair_id_for`(market_type별), `fee_bps_for`(V1/V2_CURVE/V2_DEX + fee_config None).
- 컨버터 테스트: V2_DEX swap(LVMON quote)에서 asset0/1·reserves·priceNative·pairId 정확성; V1 회귀(WMON 결과 불변).
- 통합: 시드 데이터로 `/pair`·`/events` 스냅샷.
- `cargo test -race` 범위는 terminal 모듈로 좁혀 실행.

## 10. 산출물 / 롤아웃

- 외부 핸드오프: `docs/terminal-api.md` V2 전면 갱신 (dexKey 4종·feeBps·quote 기준·한계). Notion 동일 갱신 필요(별도 안내).
- 내부 설계: 본 문서.
- `chang.md` → 구현 후 `complete.md` 이전.
- 비파괴적, 마이그레이션 불필요(quote_id 컬럼 기존재). `/codex review` 후 v2 브랜치 기준 PR.
