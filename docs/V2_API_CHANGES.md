# nad.fun API V2 — 인터페이스 변경사항

현재 mainnet(V1) API와 V2 릴리즈 간의 모든 API 응답 스키마 변경사항을 정리한 문서입니다. 타입/필드 변경만 기술합니다.

---

## 요약

| 타입 | 변경 |
|---|---|
| `TokenInfo` | `hackathon_info` 제거, `version` 추가 |
| `MarketInfo` | `quote_info` (nested), `reserve_quote`, `quote_price`, `price_quote`, `ath_price_quote` 추가 |
| `QuoteInfo` | 신규 타입 (`quote_id`, `name`, `symbol`, `decimals`, `image_uri`) |
| `SwapInfo` | `quote_amount`, `quote_price` 추가 |
| `MarketType` enum | `V2_CURVE`, `V2_DEX` 추가 |
| `TokenVersion` enum | 신규 타입 (`V1` \| `V2`) |
| `HackathonInfo` + 하위 타입 | 전체 제거 |
| 엔드포인트 | `/token/hackathon`, `/order/hackathon`, `/cms/hackathon/register` 제거 |

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
}
```

### 변경사항

- **제거**: `quote_id` (flat field) — `quote_info.quote_id` 로 대체됨.
- **추가**: `quote_info` — quote 토큰 메타데이터 (nested object). `quote_id`, `name`, `symbol`, `decimals`, `image_uri` 포함. `quote_token` 테이블에서 JOIN으로 가져옴.
- **추가**: `reserve_quote` — quote 자산 리저브. V1/WMON 토큰에서는 `reserve_native`와 동일값.
- **추가**: `quote_price` — quote 자산 / USD 환율. V1/WMON 토큰에서는 `native_price`와 동일값.
- **추가**: `price_quote` — quote 자산 기준 토큰 가격. V1/WMON 토큰에서는 `price_native`와 동일값.
- **추가**: `ath_price_quote` — quote 자산 기준 최고가(ATH). V1/WMON 토큰에서는 `ath_price_native`와 동일값.

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
