# Dividend API 설계

- **날짜**: 2026-06-15
- **브랜치**: `feat/dividend-api` (base: `v2`)
- **대상 화면**: Profile Dividend / Trade Dividend / Vault Dividend (3개 탭)
- **데이터 소스**: `migrations/dividend.sql` (DividendVault 인덱싱 + 스케줄러 accrual/distribution 파이프라인)

---

## 1. 배경 — 3개 화면

1. **Profile Dividend** — 지갑이 배당 받는 토큰 목록. 토큰별 Market Cap / Claimable Dividend(Claim 버튼) / Claimed Dividend(마지막 청구 시각).
2. **Trade Dividend** — 토큰 상세의 Dividend 탭. 배당토큰 비율 헤더(예: XAUt0 25%, USDT 25%) + 홀더 랭킹(Holder / Total Dividend Value / Last Received Time).
3. **Vault Dividend** — 토큰 배당 설정 카드. Dividend %(80%), Allocated Volume, Total Dividends(USD), 배당토큰별 비율·금액, Dividend Recipients, Eligibility(min_balance), Last Executed.

---

## 2. 스키마 매핑 / 파이프라인

### 2.1 온체인 인덱싱 테이블 (`v2_dividend_*`)
| 테이블 | 내용 |
|---|---|
| `v2_dividend_setups` | source별 배당토큰·`ratio`(BPS)·`min_balance` (setup 1회 불변 = config) |
| `v2_dividend_deposits` | 수수료 슬라이스 유입 (immediate vs swap-pending), `usd_value` |
| `v2_dividend_conversions` | pending→배당토큰 스왑 |
| `v2_dividend_merkle_roots` | 온체인 SetMerkleRoot 이벤트 마커 |
| `v2_dividend_claims` | **지급 완료된** claim만 (holder, amount, usd_value, time) |
| `v2_dividend_vault_stats` | (source, dividend_token) 집계 — totals deposited/converted/claimed + USD, `dividend_balance`, `claim_count` |

### 2.2 스케줄러 accrual/distribution 파이프라인 (신규)
| 테이블 | 내용 | PK |
|---|---|---|
| `dividend_accrual_history` | 스냅샷별 홀더 적립 델타(`accrued`) + snapshot_balance | (source, dividend, holder, balance_to) |
| `dividend_accrual` | **홀더별 누적 적립액**(history 합계, 트리거 집계) = 살아있는 entitlement | (source, holder, dividend) |
| `dividend_pair_state` | pair별 last_allocated_balance / snapshot_block | (source, dividend) |
| `dividend_merkle_root` | 스케줄러 발행 root + `leaf_count` (온체인 `v2_dividend_merkle_roots`와 별개) | merkle_root |
| `dividend_distribution` | **머클 leaf**: `amount`=누적 적립, `proof` 포함(claim tx용) | (root, source, holder, dividend) |

**파이프라인**: 잔고 스냅샷 → 홀더 적립 → `dividend_accrual_history` → (트리거) `dividend_accrual`+`dividend_pair_state` → 주기적 root 발행 → `dividend_merkle_root` + `dividend_distribution`(leaf+proof) → 홀더 온체인 claim → `v2_dividend_claims`.

### 2.3 외부 조인 (타 마이그레이션)
- **토큰 메타**: `token`(symbol/name/image_uri/is_graduated/creator…) → `TokenInfo`
- **마켓/가격/공급/홀더수**: `market` + price + `dex_token_price.price_usd` → `MarketInfo`. Market Cap = `price_usd × total_supply` (기존 `token_order.fetch_market_cap` 패턴 재사용, 새 컬럼 없음).
- **status**: `market.market_type` (`CURVE/DEX/V2_CURVE/V2_DEX`) / `token.is_graduated`.
- **분배율(Dividend 80% / Creator 20%)**: `v2_creator_fee_allocation(token_id, vault_id, bps)`. `vault_type`에 'DIVIDEND'가 없으므로 **dividend vault 주소(`DIVIDEND_VAULT_ADDRESS`)로 식별**.
- **자격자(Recipients)**: `balance(account_id, token_id, balance raw)` 에서 `balance ≥ min_balance` 카운트.
- **배당토큰 메타**: quote_token에 없을 수 있어 `token ∪ quote_token ∪ dex_token` union 해석(= `/dex/tokens` 방식) → `QuoteInfo`.

---

## 3. 핵심 파생값 정의 (결정 사항)

- **Claimable now (Claim 버튼, proof 포함)** = 최신 `dividend_distribution.amount`(누적 leaf) − 누적 `v2_dividend_claims.amount`, `GREATEST(_, 0)`. → **이미지 #1**. `RewardInfo { amount, claimed_amount, proof, claimable }`로 표현.
- **Total Dividend Value (이미지 #2)** = **누적 적립 기준** (`dividend_accrual.accrued`) → USD 환산(현재가 곱). _(결정: 누적적립 채택; claimed 아님)_
- **Dividend Recipients (이미지 #3)** = **자격자** = `COUNT(*) balance WHERE balance ≥ min_balance`. _(결정: 자격자 채택)_
- **Allocated Volume (이미지 #3)** = `Σ(total_deposited + total_pending_deposited)` (quote/MON raw). _(결정)_
- **Total Dividends USD** = `Σ(total_deposited_usd + total_pending_deposited_usd)`.
- **Last Executed** = `MAX(dividend_merkle_root.created_at)`.
- **Eligibility 표기** = `min_balance` + `token_info.symbol` (예: "10,000 CHOG").

> USD 가격 해석은 `dex_token_price.price_usd` + quote price fallback(= `/dex/tokens` 방식) LATERAL 조인 재사용. RewardInfo엔 USD 필드가 없어 claimable USD는 기본 미포함(FE 환산); 필요 시 옵션 확장.

---

## 4. 쿼리 설계

### 4.1 Profile Dividend (per `wallet`)
> 스키마 업데이트(서브모듈 #57): `dividend_distribution`은 **latest-only** (PK `(source_token, holder, dividend_token)`, root당 히스토리 없음) + `status('AWAITING'|'CLAIMED')` 컬럼 추가. pair별 1행이라 아래 `DISTINCT ON`은 불필요해 제거. claimable은 스냅샷 `status`(리빌드 사이 stale 가능) 대신 **live claims 기준 `amount > claimed`로 계산**.

```sql
WITH dist AS (   -- pair별 현재 root leaf (누적액 + proof), latest-only이라 1행
  SELECT source_token, dividend_token, amount AS accrued_leaf, proof
  FROM dividend_distribution
  WHERE holder = $1
),
claimed AS (
  SELECT source_token, dividend_token,
         SUM(amount) AS claimed_amount, MAX(created_at) AS last_claimed_at
  FROM v2_dividend_claims WHERE holder = $1
  GROUP BY source_token, dividend_token
)
SELECT d.source_token, d.dividend_token, d.proof,
       GREATEST(d.accrued_leaf - COALESCE(c.claimed_amount,0), 0) AS claimable,
       COALESCE(c.claimed_amount,0) AS claimed, c.last_claimed_at
FROM dist d
LEFT JOIN claimed c USING (source_token, dividend_token);
-- Rust: source_token 그룹화 → TokenInfo + MarketInfo 조인, rewards[]=배당토큰별 RewardInfo
```

### 4.2 Trade Dividend (per `source_token`, 페이지네이션)
```sql
-- 헤더 breakdown: v2_dividend_setups(ratio) + 배당토큰 메타
-- 홀더 랭킹 (누적적립 기준, USD 정렬)
SELECT a.holder,
       MAX(a.updated_at) AS last_received_at,
       a.dividend_token, a.accrued                 -- USD 환산 위해 토큰별 필요
FROM dividend_accrual a
WHERE a.source_token = $1 AND a.accrued > 0
GROUP BY a.holder, a.dividend_token, a.accrued;
-- Rust(또는 SQL CTE): 토큰별 accrued × 현재가 → holder별 합산 → total_value_usd DESC 정렬 → page/limit
-- total_count = COUNT(DISTINCT holder)
```

### 4.3 Vault Dividend (per `source_token`)
```sql
-- (A) Dividend %  : SELECT bps FROM v2_creator_fee_allocation
--                   WHERE token_id=$1 AND vault_id=$DIVIDEND_VAULT
-- (B) 배당토큰별 ratio+금액+합계 : v2_dividend_setups ⨝ v2_dividend_vault_stats (배당토큰 메타 union)
-- (C) Last Executed : SELECT MAX(created_at) FROM dividend_merkle_root
-- (D) Recipients   : SELECT COUNT(*) FROM balance WHERE token_id=$1 AND balance >= $min_balance
-- 집계(Rust): allocated_volume=Σ(total_deposited+total_pending_deposited),
--             total_dividends_usd=Σ(total_deposited_usd+total_pending_deposited_usd)
```

---

## 5. API 설계 (기존 타입 정렬)

`src/types/common/info.rs`의 `TokenInfo` / `MarketInfo` / `QuoteInfo` / `RewardInfo` / `AccountInfo` 재사용. 페이지네이션 래퍼는 기존 컨벤션 `{ <복수명>: Vec, total_count }` (page/limit 에코 X).

### 5.1 엔드포인트
| # | 화면 | Method · Path | 비고 |
|---|---|---|---|
| ① | Profile Dividend | `GET /profile/dividend/:account_id` | `/profile/hold-token/:account_id` 컨벤션 |
| ② | Trade Dividend | `GET /dividend/holders/:token_id` | 페이지네이션 |
| ③ | Vault Dividend | `GET /dividend/:token_id` | `/vault/:token_id` 미러 |
| ④ | Dividend token search | `GET /dividend/tokens?q=&page=&limit=` | 배당토큰 후보 = whitelist(enabled) ∪ V1(졸업) ∪ V2(전체). `DexTokenEntry`/`DexTokenListResponse` 재사용(balance 제외, price_usd 포함). q=name/symbol/CA substring. 정적 라우트라 `/dividend/:token_id`와 충돌 없음(matchit static 우선, /profile 선례) |

### 5.2 응답 타입 (`src/types/dividend/mod.rs`)
```rust
// ① — CreatedTokensResponse / TokenCreatedInfo 미러
pub struct DividendTokensResponse { pub tokens: Vec<DividendTokenInfo>, pub total_count: i64 }
pub struct DividendTokenInfo {
    pub token_info: TokenInfo,
    pub market_info: MarketInfo,
    pub rewards: Vec<DividendReward>,
    pub last_claimed_at: Option<i64>,
}
pub struct DividendReward { pub dividend_token_info: QuoteInfo, pub reward_info: RewardInfo }

// ② Trade
pub struct DividendHoldersResponse {
    pub token_info: TokenInfo,
    pub dividend_tokens: Vec<DividendRatioInfo>,
    pub holders: Vec<DividendHolderInfo>,
    pub total_count: i64,
}
pub struct DividendRatioInfo { pub dividend_token_info: QuoteInfo, pub ratio_bps: i32 }
pub struct DividendHolderInfo { pub holder: AccountInfo, pub total_value_usd: String, pub last_received_at: i64 }

// ③ Vault
pub struct DividendVaultResponse {
    pub token_info: TokenInfo,
    pub market_info: MarketInfo,
    pub dividend_bps: i32,
    pub allocated_volume: String,
    pub total_dividends_usd: String,
    pub min_balance: String,
    pub recipient_count: i64,
    pub last_executed_at: i64,
    pub dividend_tokens: Vec<DividendStatInfo>,
}
pub struct DividendStatInfo { pub dividend_token_info: QuoteInfo, pub ratio_bps: i32, pub amount: String, pub amount_usd: String }
```

### 5.3 컨벤션
- 숫자 → String(`BigDecimal.normalized().to_plain_string()`), raw 스케일 유지 + `decimals` 동봉(FE 스케일).
- 주소 EIP-55, 핸들러 진입 즉시 `valid_account_id` / `valid_existing_token_id`.
- 3-tier: router → service(redis 캐시 + single_flight) → controller(`measure_postgres!`, read pool).
- utoipa `#[utoipa::path]` + `ToSchema`, main.rs openapi paths/schemas 등록.

---

## 6. 신설/수정 파일
```
src/router/dividend/{mod.rs, handler.rs, path.rs}   (신설)
src/services/dividend/mod.rs                          (신설)
src/controllers/dividend/mod.rs                       (신설)
src/types/dividend/mod.rs                             (신설)
src/config.rs + .env.example : DIVIDEND_VAULT_ADDRESS (수정)
src/main.rs : .merge(dividend::router()) + openapi 등록 (수정)
```

---

## 7. 선결 조건 / 리스크
1. **`DIVIDEND_VAULT_ADDRESS`** — singleton DividendVault 주소(메인넷/테스트넷)를 env/config로 주입(80% bps 식별).
2. **마이그레이션 적용** — `migrations/dividend.sql`은 **서브모듈** → migrations feat 브랜치 커밋·push + 부모 gitlink bump. 로컬 DB 적용 후 컴파일타임 `query_as!` 사용 시 `.sqlx` 오프라인 캐시 재생성(또는 vault 컨트롤러처럼 런타임 `query_as::<_, T>` 사용).
3. **claimable USD** — `dividend_distribution`엔 USD 없음. RewardInfo 유지(미포함) 또는 가격 조인으로 옵션 확장.
4. **Trade 랭킹 USD 정렬 비용** — 배당토큰별 가격 조인 후 holder 합산 → 정렬. 캐시 TTL 설정 권장.
