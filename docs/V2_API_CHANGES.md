# nad.fun API V2 — 인터페이스 변경사항

현재 mainnet(V1) API와 V2 릴리즈 간의 모든 API 응답 스키마 변경사항을 정리한 문서입니다. 타입/필드 변경만 기술합니다.

---

## X Verification — Followed-by 최소 팔로워 수 + 팔로워 수 반올림 (신규)

`POST /x/followed-by`가 창작자를 팔로우하는 핸들이라도 팔로워 수가 `X_FOLLOWED_BY_MIN_FOLLOWERS`(기본값 1000) 미만이면 `400 insufficient_followers`로 거부. **창작자 본인의 `followers_count`만** **API 반환 시점에** 1,000 단위로 내림(0–999 → 0, 1000–1999 → 1000, …) — **DB엔 정확한 원본 저장**, status/xinfo/token embed 응답에서만 내림. `followed_by[].x_followers_count`(제3자, 이미 공개)는 **원본 유지**. `is_x_verified`는 레거시 `verified`뿐 아니라 **`verified_type`(X Premium/Blue·business·government)**도 반영해 판정(구 필드만 보면 최신 인증 계정이 전부 false로 나오던 버그 수정). 상세는 [`x-verification-api.md`](./x-verification-api.md) 참고.

---

## 요약

| 타입 | 변경 |
|---|---|
| `TokenInfo` | `hackathon_info` 제거, `version` 추가 |
| `MarketInfo` | `quote_info` (nested), `reserve_quote`, `quote_price`, `price_quote`, `ath_price_quote`, `fee_info` (optional) 추가 |
| `QuoteInfo` | 신규 타입 (`quote_id`, `name`, `symbol`, `decimals`, `image_uri`) |
| `FeeInfo` | 신규 타입 (`creator_protocol_fee_rate`, `curve_protocol_fee_rate`, `dex_protocol_fee_rate`) — V2 토큰만 |
| `SwapInfo` | `quote_amount`, `quote_price` 추가 |
| `MarketType` enum | `V2_CURVE`, `V2_DEX` 추가 |
| `TokenVersion` enum | 신규 타입 (`V1` \| `V2`) |
| `MineSaltRequest` | `version` 필드 타입 변경 (`number` → `TokenVersion`) |
| `HackathonInfo` + 하위 타입 | 전체 제거 |
| 엔드포인트 | `/token/hackathon`, `/order/hackathon`, `/cms/hackathon/register` 제거 |
| 엔드포인트 | `GET /vault/{token_id}` 신규 — 토큰별 vault 분배 + stats |
| `TokenVaultsResponse`, `VaultEntry`, `VaultStats` (tagged union), `BurnStats` / `LpStats` / `CreatorFeeStats` / `GiftStats` / `EmptyStats`, `VaultType` enum | 신규 타입 — vault 응답 |
| 엔드포인트 | `GET /quote_token` 신규 — 등록된 quote token 카탈로그 |
| `QuoteTokensResponse` | 신규 타입 — quote token 목록 응답 |
| 엔드포인트 | `GET /profile/gift-fee/{account_id}` 신규 — gift vault receiver로 등록된 토큰 목록. V2 reward 출처는 `v2_gift_vault_stats` (gift 수령용 잔액) |
| `GiftFeeTokensResponse` | 신규 타입 — `TokenCreatedInfo[]` + `total_count` |
| `RewardInfo` (소스 분기) | V2 토큰의 경우 `v2_creator_fee_vault_stats` 에서 산출 (V1은 기존 `creator_reward` Merkle) — 타입 형태는 그대로 |

---

## TokenInfo

### 이전 (V1)

```typescript
interface TokenInfo {
    token_id: string;
    name: string;
    symbol: string;
    image_uri: string;
    description: string | null;
    is_graduated: boolean;
    is_nsfw: boolean;
    twitter: string | null;
    telegram: string | null;
    website: string | null;
    created_at: number;
    creator: AccountInfo;
    is_cto: boolean;
    hackathon_info: HackathonInfo | null;   // ← V2에서 제거됨
}
```

### 이후 (V2)

```typescript
interface TokenInfo {
    token_id: string;
    name: string;
    symbol: string;
    image_uri: string;
    description: string | null;
    is_graduated: boolean;
    is_nsfw: boolean;
    twitter: string | null;
    telegram: string | null;
    website: string | null;
    created_at: number;
    creator: AccountInfo;
    is_cto: boolean;
    version: TokenVersion;                   // ← 신규: "V1" | "V2"
}
```

### 변경사항

- **제거**: `hackathon_info` — 모든 응답에서 해당 필드와 하위 `HackathonInfo` 타입이 제거됨
- **추가**: `version` — 토큰 버전 (`"V1"` 또는 `"V2"`)

---

## MarketInfo

### 이전 (V1)

```typescript
interface MarketInfo {
    market_type: MarketType;
    token_id: string;
    market_id: string;
    reserve_native: string;
    reserve_token: string;
    token_price: string;
    native_price: string;
    price: string;
    price_usd: string;
    price_native: string;
    total_supply: string;
    volume: string;
    ath_price: string;
    ath_price_usd: string;
    ath_price_native: string;
    holder_count: number;
}
```

### 이후 (V2)

```typescript
interface QuoteInfo {                       // ← 신규 타입
    quote_id: string;
    name: string;
    symbol: string;
    decimals: number;
    image_uri: string;
}

interface FeeInfo {                        // ← 신규 타입 (V2 토큰만)
    creator_protocol_fee_rate: number;
    curve_protocol_fee_rate: number;
    dex_protocol_fee_rate: number;
}

interface MarketInfo {
    market_type: MarketType;
    token_id: string;
    quote_info: QuoteInfo;        // ← 신규 (nested object)
    market_id: string;
    reserve_native: string;
    reserve_quote: string;        // ← 신규
    reserve_token: string;
    token_price: string;
    native_price: string;
    quote_price: string;          // ← 신규
    price: string;
    price_usd: string;
    price_native: string;
    price_quote: string;          // ← 신규
    total_supply: string;
    volume: string;
    ath_price: string;
    ath_price_usd: string;
    ath_price_native: string;
    ath_price_quote: string;      // ← 신규
    holder_count: number;
    fee_info: FeeInfo | null;     // ← 신규 (V2 토큰만, V1은 null)
}
```

### 변경사항

- **제거**: `quote_id` (flat field) — `quote_info.quote_id` 로 대체됨.
- **추가**: `quote_info` — quote 토큰 메타데이터 (nested object). `quote_id`, `name`, `symbol`, `decimals`, `image_uri` 포함. `quote_token` 테이블에서 JOIN으로 가져옴.
- **추가**: `reserve_quote` — quote 자산 리저브. V1/WMON 토큰에서는 `reserve_native`와 동일값.
- **추가**: `quote_price` — quote 자산 / USD 환율. V1/WMON 토큰에서는 `native_price`와 동일값.
- **추가**: `price_quote` — quote 자산 기준 토큰 가격. V1/WMON 토큰에서는 `price_native`와 동일값.
- **추가**: `ath_price_quote` — quote 자산 기준 최고가(ATH). V1/WMON 토큰에서는 `ath_price_native`와 동일값.
- **추가**: `fee_info` — 수수료 설정. `fee_config` 테이블에서 JOIN. V2 토큰은 `FeeInfo` 객체, V1 토큰은 `null` 반환. `creator_protocol_fee_rate`, `curve_protocol_fee_rate`, `dex_protocol_fee_rate` 포함 (SMALLINT, basis points).

---

## SwapInfo

### 이전 (V1)

```typescript
interface SwapInfo {
    event_type: SwapType;
    native_amount: string;
    token_amount: string;
    native_price: string;
    value: string;
    transaction_hash: string;
    created_at: number;
}
```

### 이후 (V2)

```typescript
interface SwapInfo {
    event_type: SwapType;
    native_amount: string;
    quote_amount: string;         // ← 신규
    token_amount: string;
    native_price: string;
    quote_price: string;          // ← 신규
    value: string;
    transaction_hash: string;
    created_at: number;
}
```

### 변경사항

- **추가**: `quote_amount` — 스왑에 사용된 quote 자산 수량. V1/WMON 토큰에서는 `native_amount`와 동일값.
- **추가**: `quote_price` — 스왑 시점 quote 자산 / USD 환율. V1/WMON 토큰에서는 `native_price`와 동일값.

---

## MarketType (enum)

### 이전 (V1)

```typescript
type MarketType = "CURVE" | "DEX";
```

### 이후 (V2)

```typescript
type MarketType = "CURVE" | "DEX" | "V2_CURVE" | "V2_DEX";
```

### 변경사항

- **추가**: `"V2_CURVE"` — V2 본딩 커브 마켓
- **추가**: `"V2_DEX"` — V2 졸업 DEX 마켓

참고: `V2_CURVE` / `V2_DEX` 값은 DB에 V2 토큰이 생성된 이후에만 응답에 나타남. 현재 mainnet 데이터는 전부 V1.

---

## TokenVersion (신규 enum)

### V2에서 신규 추가

```typescript
type TokenVersion = "V1" | "V2";
```

`TokenInfo.version`의 값으로 사용됨. 현재 mainnet의 모든 토큰은 `version: "V1"`.

---

## MineSaltRequest (`POST /token/salt`)

### 이전 (V1)

```typescript
interface MineSaltRequest {
    creator: string;
    name: string;
    symbol: string;
    metadata_uri: string;
    version?: number;        // 1 | 2, default: 1
}
```

### 이후 (V2)

```typescript
interface MineSaltRequest {
    creator: string;
    name: string;
    symbol: string;
    metadata_uri: string;
    version?: TokenVersion;  // "V1" | "V2", default: "V1"
}
```

### 변경사항

- `version` 필드 타입이 숫자(`1`/`2`)에서 `TokenVersion` 문자열(`"V1"`/`"V2"`)로 변경됨
- 내부 상수(`V1_BONDING_CURVE`, `V2_BONDING_CURVE`)와 `TokenInfo.version`의 표기와 일관됨
- 생략 시 기본값은 `"V1"`

---

## Vault 엔드포인트 (신규)

V2에서 도입된 vault 분배 시스템(Buyback & Burn / LP Support / Creator / Gift)을
조회하기 위한 신규 엔드포인트. 상세 스펙은 [`vault-api.md`](./vault-api.md) 참고.

### `GET /vault/{token_id}`

토큰이 거래 수수료를 라우팅하는 모든 vault 목록과 비율(bps), vault 별 누적 stats를 반환.

**캐시**: 30초 (env `GET_TOKEN_VAULTS_RESPONSE_EXPIRATION` 으로 조정, ms 단위)

**응답 모양** — 공통 필드 + `vault_type` 디스크리미네이터 + `stats` payload:

```typescript
type VaultType = "BURN" | "LP" | "CREATOR_FEE" | "GIFT" | "CUSTOM";

interface VaultEntryBase {
    vault_id: string;
    bps: number;            // 0..10000
    name: string;
    active: boolean;
    quote_amount: string;     // 누적 분배 fee (quote 단위, raw wei)
    quote_amount_usd: string; // 누적 분배 fee (USD, 분배 시점 가격으로 누적)
    last_executed_at: number;
}

type VaultEntry = VaultEntryBase & (
    | { vault_type: "BURN";        stats: BurnStats }
    | { vault_type: "LP";          stats: LpStats }
    | { vault_type: "CREATOR_FEE"; stats: CreatorFeeStats }
    | { vault_type: "GIFT";        stats: GiftStats }
    | { vault_type: "CUSTOM";      stats: Record<string, never> }
);

interface TokenVaultsResponse {
    token_id: string;
    quote_id: string;                  // quote_amount 단위 (market.quote_id)
    total_quote_amount: string;        // SUM of vaults[].quote_amount
    total_quote_amount_usd: string;    // SUM of vaults[].quote_amount_usd
    vaults: VaultEntry[];              // bps DESC 정렬
}
```

**USD 필드 시점 의미**:
- 누적값(`*_deposited_usd`, `*_claimed_usd`, `*_expired_usd`, `quote_spent_usd`, `quote_injected_usd`, `buyback_quote_spent_usd`, `quote_amount_usd`, `total_quote_amount_usd`)은 **각 이벤트 시점**의 USD 환산을 누적. 인덱서가 `usd_value`를 이벤트 row에 박고 트리거가 합산.
- `current_balance_usd` (CREATOR_FEE/GIFT)는 **요청 시점**에 `current_balance × price.price / 10^decimals`로 동적 계산. price 테이블에 가격 row가 없으면 `0`.

vault_type 별 `stats` 필드 정의는 [`vault-api.md`](./vault-api.md#vault_type-별-stats) 참고.

### 데이터 출처

- 멤버십 + bps: `v2_creator_fee_allocation` JOIN `v2_vault_metadata`
- 통계: `v2_burn_vault_stats` / `v2_lp_vault_stats` / `v2_creator_fee_vault_stats` / `v2_gift_vault_stats`
- LP `pool_pair`: `token.symbol` + `quote_token.symbol` (via `market.quote_id`)
- 분배 fee 누적: `v2_creator_fee_distribution_stats` (per `(token_id, vault_id)`)
  - 트리거가 `v2_creator_fee_distribution`의 `event_type='DISTRIBUTE'` 행을 누적
  - `total_quote_amount` = 토큰 행의 `quote_amount` 합 (응답 루트)
  - `quote_id`는 `market.quote_id` 기준 (vault 0개 토큰도 단위 명시)

V1 토큰에는 vault allocation이 없으므로 `vaults: []` 빈 배열을 반환.

---

## Quote Token 엔드포인트 (신규)

V2부터 토큰별로 다양한 quote 자산을 지원하므로, quote 카탈로그를 클라이언트가
조회할 수 있도록 신규 엔드포인트 도입. 상세 스펙은 [`quote-token-api.md`](./quote-token-api.md) 참고.

### `GET /quote_token`

등록된 모든 quote token의 메타데이터(`quote_id`, `name`, `symbol`, `decimals`, `image_uri`)
배열을 반환. `pyth_feed_id`는 응답에서 제외 (서버 내부용).

**캐시**: 5분 (env `GET_QUOTE_TOKENS_RESPONSE_EXPIRATION` 으로 조정, ms 단위)

```typescript
interface QuoteInfo {
    quote_id: string;
    name: string;
    symbol: string;
    decimals: number;
    image_uri: string;
}

interface QuoteTokensResponse {
    quote_tokens: QuoteInfo[];   // created_at ASC (등록 순)
}
```

`QuoteInfo`는 `MarketInfo.quote_info` (V2)에서 사용되는 nested 타입과 동일.

---

## Profile Gift Fee 엔드포인트 (신규)

V2 gift vault의 receiver로 바인딩된 사용자가 자기가 받기로 된 토큰 목록을
profile 화면에서 조회할 수 있도록 신규 엔드포인트 추가. 상세 스펙은
[`profile-api.md`](./profile-api.md#4-gift-fee-토큰-조회-get-profilegift-feeaccount_id) 참고.

### `GET /profile/gift-fee/{account_id}`

`v2_gift_vault_stats.receiver = account_id` 인 토큰 목록을 페이지네이션으로 반환.
응답 항목은 기존 `tokens/created` 와 동일한 `TokenCreatedInfo` 형식이라 UI 카드 레이아웃 재사용 가능.

**캐시**: 30초 (env `GET_GIFT_FEE_RESPONSE_EXPIRATION` 으로 조정, ms 단위)

```typescript
interface GiftFeeTokensResponse {
    tokens: TokenCreatedInfo[];   // current_balance DESC, updated_at DESC 정렬
    total_count: number;
}
```

정렬 기준은 `current_balance` DESC (받을 수 있는 잔액 많은 순) → `updated_at` DESC (최근 활동 우선) 보조 정렬. `(receiver, current_balance DESC)` 부분 인덱스(`vault.sql`)가 WHERE + ORDER BY 모두 cover.

---

## RewardInfo (소스 분기, 타입 동일)

`TokenCreatedInfo.reward_info` 가 채워지는 방식이 토큰 버전에 따라 달라짐.
**타입(`RewardInfo`) 형태는 V1/V2 동일** — 클라이언트가 별도 분기 로직을
넣을 필요 없음. 단, `proof` 의 의미는 토큰 버전에 따라 다름 (아래 표 참고).

### 적용 엔드포인트
- `GET /profile/tokens/created/{account_id}` — **창작자(creator) 보상**
- `GET /profile/gift-fee/{account_id}` — **gift 수령자(receiver) 보상**

### 필드 매핑

V1은 두 엔드포인트 모두 동일하지만, V2는 엔드포인트별로 출처 테이블이 다릅니다 — "이 사용자가 받아야 할 돈"이 토큰 컨텍스트에 따라 다른 vault에 들어있기 때문.

| `RewardInfo` 필드 | V1 (Merkle 보상) | V2 — `/tokens/created` (CreatorFeeVault) | V2 — `/gift-fee` (GiftVault) |
|---|---|---|---|
| `amount` | `creator_reward.amount` | `v2_creator_fee_vault_stats.current_balance` | `v2_gift_vault_stats.current_balance` |
| `claimed_amount` | `SUM(creator_treasury_claim_history.amount)` | `v2_creator_fee_vault_stats.total_claimed` | `v2_gift_vault_stats.total_claimed` |
| `proof` | Merkle proof 배열 | `[]` | `[]` |
| `claimable` | `creator_reward.status === 'AWAITING'` | `current_balance > 0` | `current_balance > 0` |

### 분기 기준

1. **엔드포인트 단계**: URL 자체가 출처 vault를 결정 (`/tokens/created` ↔ CreatorFeeVault, `/gift-fee` ↔ GiftVault).
2. **토큰 version 단계**: 동일 엔드포인트 안에서 각 row의 `token.version`이 V1이면 Merkle 소스, V2면 위 vault stats로 매핑.

V1/V2 토큰이 한 응답에 섞여 있어도 각 토큰의 version 에 따라 올바른 source 가 적용됨.

---

## HackathonInfo (제거)

### 이전 (V1)

`TokenInfo.hackathon_info`는 해커톤 등록 토큰의 팀, 프로젝트, 등록 메타데이터를 담은 optional 중첩 객체였음. 전체 타입 트리(`HackathonInfo`, `HackathonTeamInfo`, `HackathonProjectInfo` 등)가 V1의 `src/types/hackathon/`에 정의되어 있었음.

### 이후 (V2)

모든 `hackathon_*` 타입이 제거됨. `TokenInfo.hackathon_info`는 더 이상 존재하지 않음. 이 필드를 읽는 클라이언트는 `undefined`를 받게 됨.

---

## 제거된 엔드포인트

다음 엔드포인트는 V2에서 제거되며 `404`를 반환:

| 메서드 | 경로 | 용도 |
|---|---|---|
| GET | `/token/hackathon` | 해커톤 토큰 목록 |
| GET | `/order/hackathon` | 해커톤 토큰 정렬 |
| POST | `/cms/hackathon/register` | 해커톤 토큰 등록 |

이 엔드포인트를 호출하는 클라이언트는 해당 호출을 제거해야 함. 대체 엔드포인트는 제공되지 않음.

---

## 필드명 vs. 의미

`native_*` 필드명(`native_price`, `native_amount`, `reserve_native`)은 하위 호환을 위해 유지됨. V1/WMON 토큰에서는 기존과 동일한 의미. V2 non-WMON quote 토큰(향후 등장 시)에서는 이 필드들이 MON 값이 아닌 quote 자산 값을 담게 됨 — `quote_*` 필드가 앞으로의 canonical source.

현재 mainnet 데이터가 전부 V1/WMON이므로 이 의미 변화는 아직 관측되지 않음.

---

## BalanceInfo — `total_balance` 추가 + 지갑∪LP 합집합 전환

### 변경 개요

`BalanceInfo` 타입에 `total_balance` 필드가 추가되고, 6개 보유 조회 엔드포인트가 **지갑 잔액∪LP 포지션 합집합(union)** 방식으로 전환됨. LP-only 행(지갑 잔액 = 0, LP 포지션 > 0)도 결과에 포함됨.

### BalanceInfo 타입 변경

```typescript
interface BalanceInfo {
    balance: string;        // 지갑 보유 잔액 (변경 없음)
    lp_balance: string;     // LP 포지션 환산 잔액 (변경 없음)
    total_balance: string;  // ← 신규: balance + lp_balance (동일 단위)
    token_price: string;
    native_price: string;
    quote_price: string;
    created_at: number;     // LP-only 행은 0
}
```

`total_balance = balance + lp_balance` (같은 십진 스케일). 홀딩 리스트의 정렬 기준으로 사용됨.

### 영향받는 엔드포인트

| 메서드 | 경로 | 기존 정렬 | 변경 후 정렬 |
|---|---|---|---|
| GET | `/profile/hold-token/{id}` | `balance DESC` | `total_balance DESC` |
| GET | `/agent/holdings/{id}` | `balance DESC` | `total_balance DESC` |
| GET | `/trade/holder/{token_id}` | `balance DESC` | `total_balance DESC` |
| GET | `/profile/tokens/created/{id}` | `created_at DESC` | `created_at DESC` (유지) |
| GET | `/agent/token/created/{id}` | `created_at DESC` | `created_at DESC` (유지) |
| GET | `/profile/gift-fee/{id}` | `gift_current_balance DESC NULLS LAST` | `gift_current_balance DESC NULLS LAST` (유지) |

### 행 집합 변경 (LP-only 행 포함)

기존에는 `balance > 0` 인 행만 반환했으나, 이제 다음 세 소스의 합집합(UNION distinct)을 반환:

1. **지갑 잔액** — `balance > 0` 인 행
2. **V2 LP 포지션** — `lp_position` 테이블에 기록된 Capricorn V2 풀 포지션 (`lp_position.balance > 0`)
3. **V1 LP 포지션** — Capricorn GraphQL API에서 조회한 포지션 (EIP-55 체크섬 변환 후 SQL JOIN)

`total_count` 도 합집합 distinct 기준으로 재계산됨.

### V1 LP 주입 동작

- **소스**: Capricorn GraphQL (`cached_fetch_by_owner` / `cached_fetch_by_token`)
- **주소 변환**: Capricorn이 반환하는 소문자 주소를 EIP-55 체크섬 형식으로 변환한 뒤 SQL JOIN (`LOWER()` 미사용)
- **실패 처리(fail-to-empty)**: Capricorn API 호출 실패 시 V1 LP 행을 제외하고 응답 200을 유지함 (지갑∪V2 합집합만 반환). 에러를 전파하지 않음.
- **cap**: 요청당 Capricorn LP 행이 2,000개를 초과하면 V1 LP 주입을 생략하고 지갑∪V2만 반환 (`tracing::warn!` 로그 기록).

### FE 주의사항

- **created / gift-fee 엔드포인트**: creator / receiver가 아닌 계정이어도 LP 포지션이 있으면 해당 토큰이 결과에 포함됨 (의도된 동작). LP-only 행은 `balance_info.created_at = 0`.
- **`total_count` 증가**: LP-only 행이 포함되므로 이전보다 `total_count`가 클 수 있음.
- **단위 일관성 가정**: `balance`, V2 풀 리저브 (`reserve0/1 / total_supply × lp.balance`), V1 Capricorn `amountHuman` 모두 동일한 십진 스케일로 가정. `total_balance = balance + lp_balance` 는 같은 단위의 합산. 실측 검증 권장 (V1 토큰 대상).

### 요약표 업데이트

| 타입/필드 | 변경 |
|---|---|
| `BalanceInfo.total_balance` | **신규 필드** (`balance + lp_balance`, 동일 단위) |
| 6개 엔드포인트 행 집합 | `balance > 0` 전용 → 지갑∪V2 LP∪V1 LP 합집합 (LP-only 행 포함) |
| `total_count` | 합집합 distinct 재계산 |
| hold-token / holder / holdings 정렬 | `balance DESC` → `total_balance DESC` |
| created / gift-fee 정렬 | 변경 없음 (`created_at DESC` / gift balance `DESC NULLS LAST`) |

---

## Select Token 모달 — `GET /dex/tokens` 단일화

Swap "Select Token" 모달용 토큰 리스팅/검색을 `GET /dex/tokens` 하나로 통합. `GET /dex/search`는 **제거됨** (라우터·핸들러·`DexSearchQuery`·OpenAPI 등록 전부 삭제). 검색은 `GET /dex/tokens?q=` 사용.

> **2026-06-04 업데이트** (branch `feat/dex-cache-policy`):
> - `price_usd` 소스를 `dex_token_price` 뷰(deepest-TVL 풀 per-token USD) 우선 + market fallback으로 전환 → transitive pure-DEX 토큰도 가격 노출.
> - `token_type`에 **`nadfun_v1`** 추가 (테이블 멤버십 분류). V1 토큰이 더 이상 external로 오분류되지 않음.
> - 검색을 `token∪dex_token∪quote_token∪whitelist`로 확장 → **V1·external이 name/symbol·partial-CA 검색에 노출**(이전엔 full-CA exact만). 기본 리스트는 무변경.
> - full-CA 미발견 시 **RPC 온체인 메타 fallback**으로 external 1건 반환.

> **2026-06-08 업데이트** (branch `feat/defillama-whitelist-pricing`):
> - **신규 엔드포인트 `GET /dex/reserves?pool_id=`** — 온체인 `lpPair.getReserves()` 백엔드 드롭인. 인덱서 `pool` 행에서 리저브를 반환하므로 **RPC 없음**. 응답: `{ pool_id, token0, token1, reserve0, reserve1, block_number }` (reserve는 raw wei, token0/1 순서 일치). 풀 없으면 404.
> - **`/dex/pools/{pool_id}`·`/dex/positions/{account_id}` 토큰 메타(symbol/decimals/image_uri) 소스에 `whitelist_token` 우선 추가** — `COALESCE(whitelist_token → token → dex_token → quote_token)`. 화이트리스트 페어(USDT/WMON 등)가 registry의 빈 `image_uri`(`quote_token`은 NOT NULL이라 `''` 저장 → 기존 COALESCE가 빈 문자열에서 멈춤) 대신 큐레이션된 아이콘·심볼로 노출. (이전 "두 엔드포인트 변경 없음" 명시를 갱신.)
> - 화이트리스트 가격 소스 **Pyth → DefiLlama** 전환 완료(주소 기반). `/dex/tokens` `price_usd`의 whitelist 경로 포함.

### 기존 → 변경 (dex 엔드포인트 영향 범위)

이번 변경은 `/dex/tokens`, `/dex/search` 두 개만 건드립니다. `/dex/positions/{account_id}`, `/dex/pools/{pool_id}`는 **변경 없음**.

#### `GET /dex/tokens`

| 항목 | 기존 (현행 v2) | 변경 (this PR) |
|---|---|---|
| 후보 집합 | `pool`의 token0/token1 **전부**(external 포함) | **화이트리스트 + nadfun V2만** (external/V1 제외) |
| 정렬 | 마켓캡 desc **평면** | **4-tier** (보유/미보유 × 화이트리스트/V2) |
| 검색 | 불가 (검색은 `/dex/search` 별도) | `q` 파라미터로 **통합** (prefix/CA) |
| 인증/계정 | 무인증, `?account=` 옵션(잔고) | 동일 (무인증, `?account=` 옵션) |
| 응답 필드 | `token_id, symbol, name, decimals, image_uri, balance?, market_cap_usd` | **+ `token_type`, `balance_usd`, `price_usd`** (`market_cap_usd` 제거 — 정렬 내부값) |
| 페이지네이션 | `page`/`limit`, `total_count` | 동일 (`total_count`는 후보 집합 기준 재계산) |

#### `GET /dex/search` — **제거됨**

| 항목 | 기존 (현행 v2) | 변경 (this PR) |
|---|---|---|
| 상태 | 활성 | **제거** — 라우트/핸들러/`DexSearchQuery`/OpenAPI 등록 전부 삭제 |
| 대체 | — | `GET /dex/tokens?q=` (검색 로직이 통합 엔드포인트에 흡수됨) |

> **FE 영향:** `/dex/search` 호출은 이제 **404**. `/dex/tokens?q=`로 전환 필수. 기존 `/dex/search` 대비 매칭이 substring→prefix로 좁아지고 external은 full CA exact일 때만 노출되며, 세션 대신 `?account=` 옵션으로 잔고를 첨부한다.

### 엔드포인트

```
GET /dex/tokens?account=0x..&q=..&page=1&limit=50
```

- `account` (optional, EIP-55): 있으면 wallet-connected → 보유 티어링 + `balance_usd` 첨부.
- `q` (optional): 없으면 4-티어 기본 리스트, 있으면 검색.
- 인증 레이어 없음 (잔고는 온체인 공개 정보).

### 기본 리스트 (q 없음) — 4-티어 우선순위

후보 = **화이트리스트** (`whitelist_token`) ∪ **Nadfun V2** (`token.version='V2'` 이면서 pool 보유). external(dex_token-only) / V1 토큰은 기본 리스트에서 제외.

| tier | 그룹 | 정렬 |
|---|---|---|
| 1 | 보유 화이트리스트 | `balance_usd` DESC |
| 2 | 보유 Nadfun V2 | `balance_usd` DESC |
| 3 | 미보유 화이트리스트 | 화이트리스트 고정순서 (MON, WMON, USDC, USDT, LVMON) |
| 4 | 미보유 Nadfun V2 | 마켓캡 DESC |

`account` 없으면 보유 티어(1,2)가 비어 화이트리스트(고정순서) → Nadfun V2(마켓캡) 순서 = wallet-not-connected.

### 검색 (q 있음)

case-insensitive **prefix** 매칭 (기존 substring → prefix로 변경).

검색 후보집합은 기본 리스트(화이트리스트+Nadfun V2)와 **다르다** — `token`(V1·V2) ∪ `dex_token`(external) ∪ `quote_token` ∪ `whitelist_token` 전체를 대상으로 한다. 즉 **V1·external 토큰은 검색에서만 노출**(기본 리스트엔 여전히 제외).

| q 형태 | 동작 |
|---|---|
| 텍스트 (0x 아님) | `symbol`/`name` prefix — `token`(V1·V2) ∪ `dex_token`(external) ∪ `quote_token` ∪ 화이트리스트 |
| `0x` + full CA (42자) | `token_id` exact, **모든 토큰**. DB 어느 테이블에도 없으면 **RPC 온체인 메타(name/symbol/decimals)로 external 1건 fallback** |
| `0x` partial | `token_id` prefix — full CA와 동일 테이블 집합 (external 포함) |

> 정책 변경: 이전엔 external이 full-CA exact일 때만 노출됐으나, 이제 텍스트/partial-CA 검색도 `dex_token`(indexed external)을 포함한다.

### `DexTokenEntry` (응답 항목)

```typescript
interface DexTokenEntry {
    token_id: string;
    symbol: string;
    name: string;
    decimals: number;
    image_uri: string;
    token_type: "whitelist" | "nadfun_v2" | "nadfun_v1" | "external";  // FE 렌더 분기. 테이블 멤버십: whitelist_token→whitelist, token.version V2/V1→nadfun_v2/nadfun_v1, 그 외→external. (external 판정 = token_type==="external")
    balance: string | null;      // raw wei. 보유(balance>0) + account 제공 시에만, 그 외 null
    balance_usd: string | null;  // 보유분 USD = balance/10^decimals × price_usd. 미보유면 null
    price_usd: string | null;    // 토큰 1개당 USD 단가, account 무관·8자리 truncate. V2/external=dex_token_price 뷰(deepest-TVL 풀 per-token USD) 우선·없으면 market×quote fallback, whitelist=DefiLlama
}
// 응답 루트: { tokens: DexTokenEntry[]; total_count: number }
//
// 참고 — 응답에 없는 파생/내부값 (FE에서 계산):
//   is_external  → token_type === "external"
//   is_held      → balance != null
//   market_cap_usd / tier → 서버 정렬 내부값일 뿐 응답에 포함되지 않음
```

### 백엔드/인덱서 의존

- 신규 `whitelist_token` 테이블 (`token_id`, `sort_order`, `enabled`). seed: MON, LVMON (WMON/USDC/USDT는 온체인 주소 확정 후 추가).
- 화이트리스트 렌더 메타는 `token`/`dex_token`/`quote_token` LEFT JOIN으로 채움 → 5개 주소가 셋 중 하나엔 row가 있어야 symbol/image 노출.

### 요약표 업데이트

| 타입/필드 | 변경 |
|---|---|
| `DexTokenListQuery.q` | **신규** optional 검색어 (4-티어 기본 vs prefix/CA 검색 분기) |
| `DexTokenEntry` | `token_type`, `balance_usd`, `price_usd` **신규 필드** (is_external/is_held/tier/market_cap_usd는 응답 미포함 — FE 파생/서버 내부값) |
| `GET /dex/tokens` | 평면 마켓캡 정렬 → 4-티어(보유/미보유 × 화이트리스트/V2) + 검색 통합 |
| `GET /dex/search` | **제거** → `GET /dex/tokens?q=` 사용 (404) |
| `whitelist_token` 테이블 | **신규** — 고정순서 화이트리스트 |

---

## CMS — 화이트리스트 토큰 관리

admin이 `whitelist_token`(Select Token 4-tier 화이트리스트)을 런타임에 추가/수정/숨김/조회. 마이그레이션 하드코딩 없이 WMON/USDC/USDT 등 등록.

### `POST /cms/whitelist-token` (admin)

upsert(추가/수정/소프트삭제). `authenticate_user` 세션 + admin 인가(write pool, 원자 가드).

요청 `WhitelistTokenUpsertRequest`:
```typescript
interface WhitelistTokenUpsertRequest {
    token_id: string;      // external 허용 → valid_account_id(순수 체크섬). valid_token_id(베니티) 아님
    sort_order: number;    // 모달 고정 순서(작을수록 위)
    enabled?: boolean;     // 생략 시 true. 소프트삭제 = false
}
```
- `token_id`가 `token`/`dex_token`/`quote_token` 중 어디에도 없으면 **404** (빈 메타 항목 방지).
- non-admin → **403** (기존 행도 덮어쓸 수 없음, INSERT...WHERE EXISTS(admin) 원자 가드).
- 잘못된 주소 → **400**.
- 응답: `CmsActionResponse { success: boolean }`.

### `GET /cms/whitelist-token` (admin)

전체 목록(disabled 포함), `sort_order` ASC. symbol/name/image는 `token`/`dex_token`/`quote_token` LEFT JOIN.

응답 `WhitelistTokenListResponse`:
```typescript
interface WhitelistTokenEntry {
    token_id: string;
    symbol: string;
    name: string;
    image_uri: string;
    sort_order: number;
    enabled: boolean;
}
interface WhitelistTokenListResponse { tokens: WhitelistTokenEntry[] }
```

### 요약표

| 타입/엔드포인트 | 변경 |
|---|---|
| `POST /cms/whitelist-token` | **신규** — 화이트리스트 upsert(admin). 메타 없으면 404, 소프트삭제=enabled:false |
| `GET /cms/whitelist-token` | **신규** — 화이트리스트 목록(admin, disabled 포함) |
| `WhitelistTokenUpsertRequest` / `WhitelistTokenEntry` / `WhitelistTokenListResponse` | **신규 타입** |

---

## Terminal API — V2-aware 라우팅 / 가격 산정

GeckoTerminal 호환 Terminal 엔드포인트(`GET /pair`, `GET /events`)가 V2(멀티 quote bonding curve + NadSwap DEX)를 인식하도록 변경됨. **응답 스키마(타입/필드명/JSON 키)는 V1과 100% 동일** — `Pair`, `Event`(`swap`/`join`/`exit`)의 모양은 그대로다. 값과 산정 방식만 바뀌는 **동작(behavioral) 변경**이며 타입 변경이 아니다. 상세 스펙은 [`terminal-api.md`](./terminal-api.md) 참고.

핵심 변경 (V1 토큰 응답은 불변):

- **라우팅/가격이 `quote_id` 기준**: asset 정렬(`asset0Id`/`asset1Id`)·`reserves`·`priceNative`가 WMON 하드코딩 대신 토큰의 `market.quote_id`(WMON 또는 LVMON 등) 기준으로 산정됨. `asset0`/`asset1`은 토큰 vs quote 주소를 소문자 오름차순 비교하여 정렬, `priceNative`는 quote 자산 단위.
- **`dexKey` 확장**: `nadfun-v2`(V2_CURVE), `nadswap`(V2_DEX) 추가. 기존 `nadfun`(CURVE)/`capricorn`(DEX)는 유지.
- **`feeBps` V2 fee-config 기반**: V2_CURVE = `creator_fee_rate + curve_protocol_fee_rate`, V2_DEX = `25(NadSwap LP) + creator_fee_rate + dex_protocol_fee_rate`. V1(CURVE/DEX)은 `100` 고정. V2 토큰에 `fee_config` 행이 없으면 `feeBps` 필드 생략.
- **swap `pairId` per-event 산정**: `/events`의 swap은 이벤트 자신의 `market_type`으로 pairId를 결정(졸업 토큰의 커브 시절 swap은 본딩 커브, DEX 시절은 풀 주소).

### 요약표

| 엔드포인트/항목 | 변경 |
|---|---|
| `GET /pair`, `GET /events` | **동작 변경** (스키마 불변) — `quote_id` 기준 라우팅/가격 산정 |
| `dexKey` | `nadfun-v2`(V2_CURVE), `nadswap`(V2_DEX) **값 추가** |
| `feeBps` | V2 fee-config 파생 (V2_DEX는 +25 LP). fee_config 없으면 생략 |
| swap `pairId` | 이벤트별 `market_type` 기준 산정 |
| 응답 타입 (`Pair`/`Event`) | **변경 없음** (값/동작만 변경) |

---

## Dev Post API (신규)

코인의 온체인 creator가 자신의 코인에 글(텍스트 + 이미지 + 선택적 poll)을 올리는 신규 기능. 전체 피드,
Trending(최근 7일 좋아요 상위 3, 부족하면 최신 게시물로 채움), 코인별 누적 좋아요 랭킹 세 가지 집계 뷰를
제공. 상세 스펙은 [`dev-post-api.md`](./dev-post-api.md) 참고.

### 신규 엔드포인트

| Method | Path | 인증 | 설명 |
|---|---|---|---|
| GET | `/dev-post` | optional-auth | 피드 (전체 또는 `token_id` 필터) |
| GET | `/dev-post/trending` | optional-auth | 최근 7일 좋아요 상위 3개 (부족분은 최신 게시물로 채움) |
| GET | `/dev-post/ranking` | X | 코인별 누적 좋아요 랭킹 |
| GET | `/dev-post/{post_id}` | optional-auth | 게시물 상세 |
| POST | `/dev-post/image` | O | 이미지 업로드 (raw body → R2, 5MB 제한) |
| POST | `/dev-post` | O (creator only) | 게시물 생성 |
| PATCH | `/dev-post/{post_id}` | O (author only) | 게시물 수정 (본문/이미지, poll은 불변) |
| DELETE | `/dev-post/{post_id}` | O (author only) | 소프트 삭제 |
| PUT | `/dev-post/{post_id}/pin` | O (current creator + current-creator-authored live post) | 최초/반복/교체 pin, 빈 204 |
| DELETE | `/dev-post/{post_id}/pin` | O (current creator only) | exact-post/반복 pin 해제, 빈 204 |
| POST/DELETE | `/dev-post/{post_id}/like` | O | 좋아요 토글 |
| POST | `/dev-post/{post_id}/vote` | O | 투표 (마감 전까지 변경 가능) |

`DevPostResponse.id`는 BIGINT를 문자열로 직렬화(JS 2^53 정밀도 손실 방지). `poll.is_closed`는 읽기
시점에 `closes_at <= now()`로 계산(별도 배치 없음).

### 제목/조정 계약

생성 예시는 `{"token_id":"0x...","title":"Exact title\nwith newline","body":"Description only"}`이며 제목 누락/null/blank는 400, 전체 요청 초과는 413이다. 수정은 omitted-preserve, null/blank 400의 tri-state다. 공백/줄바꿈은 보존하고 title cap은 없으며 모든 응답 surface가 title과 body를 분리한다. 구버전 reader 오표시는 허용하지만 old-create 400과 legacy body-only edit ambiguity는 허용하지 않는다. capability header, versioned path, fallback/negotiation은 없다. OpenAPI operation 수는 Dev Post 13 + CMS 2다.

### 요약표

| 타입/엔드포인트 | 변경 |
|---|---|
| `GET/POST/PUT/PATCH/DELETE /dev-post*` (13개 엔드포인트) | **신규** — Dev Post 기능 전체 |
| `DevPostResponse`, `TokenSummary`, `AuthorSummary`, `PollResponse`, `PollOptionResponse`, `RankingRow`, `LikeResponse`, `VoteResponse`, `UploadImageResponse` 등 | **신규 타입** |
| `token.market_cap` (Dev Post 응답 내) | market×price 조인 기반 USD 마켓캡으로 채워짐. **랭킹은 사람 단위 USD 정수 문자열**(소수점 버림, 예: `"15875"`), 피드/상세/트렌딩은 raw ×10^18 (후속 통일 예정) |
| Trending/Ranking Redis 캐싱 | 아직 미적용 — 매 요청 라이브 쿼리 (성능 후속 작업) |
