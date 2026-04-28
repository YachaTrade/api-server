# nad.fun API V2 — 인터페이스 변경사항

현재 mainnet(V1) API와 V2 릴리즈 간의 모든 API 응답 스키마 변경사항을 정리한 문서입니다. 타입/필드 변경만 기술합니다.

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
| 엔드포인트 | `GET /profile/gift-fee/{account_id}` 신규 — gift vault receiver로 등록된 토큰 목록 |
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
    vaults: VaultEntry[];   // bps DESC 정렬
}
```

vault_type 별 `stats` 필드 정의는 [`vault-api.md`](./vault-api.md#vault_type-별-stats) 참고.

### 데이터 출처

- 멤버십 + bps: `v2_creator_fee_allocation` JOIN `v2_vault_metadata`
- 통계: `v2_burn_vault_stats` / `v2_lp_vault_stats` / `v2_creator_fee_vault_stats` / `v2_gift_vault_stats`
- LP `pool_pair`: `token.symbol` + `quote_token.symbol` (via `market.quote_id`)

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
