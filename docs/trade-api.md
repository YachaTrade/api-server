# Trade API 문서

## 개요

Trade API는 토큰 거래 관련 정보를 조회하기 위한 API입니다.

- **Swap History**: 토큰의 거래 내역 조회
- **Holder**: 토큰 보유자 목록 조회
- **Market**: 토큰 마켓 정보 조회
- **Chart**: 토큰 가격 차트 데이터 조회
- **Metrics**: 토큰 거래 지표 조회 (거래량, 거래 수, 가격 변동률 등)

---

## API 엔드포인트

### 1. Swap History 조회 (`GET /trade/swap-history/{token_id}`)

특정 토큰의 거래 내역을 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `token_id` | string | O | 토큰 컨트랙트 주소 (EVM 형식) |

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 |
| `limit` | integer | 20 | 페이지당 항목 수 |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |
| `volume_ranges` | string | - | 거래 규모 필터 (쉼표 구분): `small` ($0-$1000), `medium` ($1000-$10000), `large` ($10000+) |
| `account_id` | string | - | 특정 계정의 거래만 필터링 |
| `trade_type` | string | ALL | 거래 타입 필터: `BUY`, `SELL`, `ALL` |

#### 응답
```json
{
  "swaps": [
    {
      "account_info": {
        "account_id": "0x...",
        "nickname": "Trader",
        "bio": "...",
        "image_uri": "https://..."
      },
      "swap_info": {
        "event_type": "BUY",
        "native_amount": "1000000000000000000",
        "token_amount": "500000000000000000000",
        "native_price": "3000",
        "value": "3000",
        "transaction_hash": "0x...",
        "created_at": 1234567890
      }
    }
  ],
  "total_count": 100
}
```

#### 에러 응답
- `400`: 잘못된 token_id 또는 파라미터
- `500`: 내부 서버 에러

---

### 2. Token Holder 조회 (`GET /trade/holder/{token_id}`)

특정 토큰의 보유자 목록을 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `token_id` | string | O | 토큰 컨트랙트 주소 (EVM 형식) |

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 |
| `limit` | integer | 20 | 페이지당 항목 수 |

#### 응답
```json
{
  "holders": [
    {
      "account_info": {
        "account_id": "0x...",
        "nickname": "Holder",
        "bio": "...",
        "image_uri": "https://..."
      },
      "balance_info": {
        "balance": "1000000000000000000000",
        "token_price": "0.001",
        "native_price": "3000",
        "created_at": 1234567890
      }
    }
  ],
  "total_count": 150
}
```

#### 에러 응답
- `400`: 잘못된 token_id
- `500`: 내부 서버 에러

---

### 3. Market 정보 조회 (`GET /trade/market/{token_id}`)

특정 토큰의 마켓 정보를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `token_id` | string | O | 토큰 컨트랙트 주소 (EVM 형식) |

#### 응답
```json
{
  "market_info": {
    "market_type": "CURVE",
    "token_id": "0x...",
    "quote_info": {
      "quote_id": "0x...",
      "name": "Wrapped MON",
      "symbol": "WMON",
      "decimals": 18,
      "image_uri": "https://..."
    },
    "market_id": "0x...",
    "reserve_native": "100000000000000000000",
    "reserve_quote": "100000000000000000000",
    "reserve_token": "500000000000000000000000",
    "token_price": "0.001",
    "native_price": "3000",
    "quote_price": "3000",
    "price": "0.000001",
    "price_usd": "0.003",
    "price_native": "0.000001",
    "price_quote": "0.000001",
    "total_supply": "1000000000000000000000000000",
    "volume": "50000000000000000000000",
    "ath_price": "0.000002",
    "ath_price_usd": "0.006",
    "ath_price_native": "0.000002",
    "ath_price_quote": "0.000002",
    "holder_count": 150
  }
}
```

#### 필드 설명

| 필드 | 설명 |
|------|------|
| `market_type` | 마켓 타입 (`CURVE`, `DEX`) |
| `token_id` | 토큰 컨트랙트 주소 |
| `quote_info` | Quote 토큰 메타데이터 (nested object) |
| `market_id` | 마켓 컨트랙트 주소 |
| `reserve_native` | 네이티브 토큰(MON) 리저브 |
| `reserve_quote` | Quote 자산 리저브 |
| `reserve_token` | 토큰 리저브 |
| `token_price` | Token/USD 가격 |
| `native_price` | MON/USD 가격 |
| `quote_price` | Quote/USD 가격 |
| `price` | MON/Token 가격 |
| `price_usd` | USD/Token 가격 |
| `price_native` | MON/Token 가격 |
| `price_quote` | Quote/Token 가격 |
| `total_supply` | 총 공급량 |
| `volume` | 총 거래량 |
| `ath_price` | ATH 가격 |
| `ath_price_usd` | ATH 가격 (USD) |
| `ath_price_native` | ATH 가격 (MON) |
| `ath_price_quote` | ATH 가격 (Quote) |
| `holder_count` | 보유자 수 |

#### 에러 응답
- `400`: 잘못된 token_id
- `500`: 내부 서버 에러

---

### 4. Chart 데이터 조회 (`GET /trade/chart/{token_id}`)

특정 토큰의 가격 차트 데이터를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `token_id` | string | O | 토큰 컨트랙트 주소 (EVM 형식) |

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `resolution` | string | 5 | 차트 해상도 - 분: `1`, `5`, `15`, `30` / 시간(분 단위): `60`, `240` / 일: `1D`, `D` |
| `from` | integer | - | 시작 타임스탬프 (초 단위, 필수) |
| `to` | integer | - | 종료 타임스탬프 (초 단위, 필수) |
| `countback` | integer | 500 | 반환할 최대 캔들 수 (최대 3000) |
| `chart_type` | string | price | 차트 타입: `price` (MON/TOKEN), `price_usd` (USD), `market_cap` (시가총액 MON), `market_cap_usd` (시가총액 USD) |

#### 응답
```json
{
  "k": "price",
  "t": [1234567800, 1234568100, 1234568400],
  "c": ["0.001", "0.0012", "0.0011"],
  "o": ["0.0009", "0.001", "0.0012"],
  "h": ["0.0011", "0.0013", "0.0012"],
  "l": ["0.0009", "0.001", "0.0011"],
  "v": ["1000", "1500", "800"],
  "s": "ok"
}
```

#### 필드 설명

| 필드 | 설명 |
|------|------|
| `k` | 차트 타입 |
| `t` | 타임스탬프 배열 (초 단위) |
| `c` | 종가 (close) 배열 |
| `o` | 시가 (open) 배열 |
| `h` | 고가 (high) 배열 |
| `l` | 저가 (low) 배열 |
| `v` | 거래량 (volume) 배열 |
| `s` | 상태 코드: `ok`, `error`, `no_data` |

#### 에러 응답
- `400`: 잘못된 token_id 또는 파라미터
- `404`: 토큰을 찾을 수 없음
- `500`: 내부 서버 에러

---

### 5. Trading Metrics 조회 (`GET /trade/metrics/{token_id}`)

특정 토큰의 거래 지표를 여러 타임프레임에 대해 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `token_id` | string | O | 토큰 컨트랙트 주소 (EVM 형식) |

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `timeframes` | string | - | 쉼표로 구분된 타임프레임 (필수). 분: `1`, `5`, `15`, `30` / 시간(분 단위): `60`, `240` / 일: `1D` |

#### 예시
```
GET /trade/metrics/0x1234...?timeframes=1,5,15,30,60,240,1D
```

#### 응답
```json
{
  "metrics": [
    {
      "timeframe": "1m",
      "percent": 2.5,
      "transactions": {
        "buy": 10,
        "sell": 5,
        "total": 15
      },
      "volume": {
        "buy": "1000000000000000000",
        "sell": "500000000000000000",
        "total": "1500000000000000000"
      },
      "makers": {
        "buy": 8,
        "sell": 4,
        "total": 12
      }
    },
    {
      "timeframe": "5m",
      "percent": 5.2,
      "transactions": {
        "buy": 50,
        "sell": 30,
        "total": 80
      },
      "volume": {
        "buy": "5000000000000000000",
        "sell": "3000000000000000000",
        "total": "8000000000000000000"
      },
      "makers": {
        "buy": 20,
        "sell": 15,
        "total": 35
      }
    }
  ]
}
```

#### 필드 설명

| 필드 | 설명 |
|------|------|
| `timeframe` | 타임프레임 (1m, 5m, 15m, 30m, 60m, 240m, 1d) |
| `percent` | 가격 변동률 (%) |
| `transactions` | 거래 횟수 (buy/sell/total) |
| `volume` | 거래량 (buy/sell/total) |
| `makers` | 유니크 거래자 수 (buy/sell/total) |

#### 에러 응답
- `400`: 잘못된 token_id 또는 timeframes
- `500`: 내부 서버 에러

---

## TypeScript Interfaces

### 요청 타입

```typescript
// GET /trade/swap-history/{token_id}
interface SwapQuery {
  page?: number;          // default: 1
  limit?: number;         // default: 20
  direction?: "ASC" | "DESC";  // default: "DESC"
  volume_ranges?: string; // "small" | "medium" | "large" (쉼표 구분 가능)
  account_id?: string;    // 계정 주소로 필터
  trade_type?: "BUY" | "SELL" | "ALL";  // default: "ALL"
}

// GET /trade/holder/{token_id}
interface PaginationParams {
  page?: number;   // default: 1
  limit?: number;  // default: 20
}

// GET /trade/chart/{token_id}
interface GetBarsRequest {
  resolution?: string;    // default: "5"
  from: number;           // 시작 타임스탬프 (초)
  to: number;             // 종료 타임스탬프 (초)
  countback?: number;     // default: 500, max: 3000
  chart_type?: "price" | "price_usd" | "market_cap" | "market_cap_usd";  // default: "price"
}

// GET /trade/metrics/{token_id}
interface MetricsQuery {
  timeframes: string;  // 쉼표 구분: "1,5,15,30,60,240,1D"
}
```

### 응답 타입

```typescript
// GET /trade/swap-history/{token_id}
interface TokenSwapResponse {
  swaps: TokenSwap[];
  total_count: number;
}

interface TokenSwap {
  account_info: AccountInfo;
  swap_info: SwapInfo;
}

interface AccountInfo {
  account_id: string;
  nickname: string;
  bio: string;
  image_uri: string;
}

interface SwapInfo {
  event_type: "BUY" | "SELL";
  native_amount: string;
  token_amount: string;
  native_price: string;
  value: string;  // 거래 시점의 USD 가치
  transaction_hash: string;
  created_at: number;
}

// GET /trade/holder/{token_id}
interface TokenHolderResponse {
  holders: TokenHolder[];
  total_count: number;
}

interface TokenHolder {
  account_info: AccountInfo;
  balance_info: BalanceInfo;
}

interface BalanceInfo {
  balance: string;
  token_price: string;
  native_price: string;
  created_at: number;
}

// GET /trade/market/{token_id}
interface MarketResponse {
  market_info: MarketInfo;
}

interface QuoteInfo {
  quote_id: string;
  name: string;
  symbol: string;
  decimals: number;
  image_uri: string;
}

interface MarketInfo {
  market_type: "CURVE" | "DEX";
  token_id: string;
  quote_info: QuoteInfo;
  market_id: string;
  reserve_native: string;
  reserve_quote: string;
  reserve_token: string;
  token_price: string;
  native_price: string;
  quote_price: string;
  price: string;
  price_usd: string;
  price_native: string;
  price_quote: string;
  total_supply: string;
  volume: string;
  ath_price: string;
  ath_price_usd: string;
  ath_price_native: string;
  ath_price_quote: string;
  holder_count: number;
}

// GET /trade/chart/{token_id}
interface BarResponse {
  k: string;       // chart type
  t: number[];     // 타임스탬프 배열 (초)
  c: string[];     // 종가 배열
  o: string[];     // 시가 배열
  h: string[];     // 고가 배열
  l: string[];     // 저가 배열
  v: string[];     // 거래량 배열
  s: "ok" | "error" | "no_data";  // 상태 코드
}

// GET /trade/metrics/{token_id}
interface MetricsBatchResponse {
  metrics: MetricItem[];
}

interface MetricItem {
  timeframe: string;  // "1m", "5m", "15m", "30m", "60m", "240m", "1d"
  percent: number;    // 가격 변동률 (%)
  transactions: TransactionCount;
  volume: VolumeAmount;
  makers: MakerCount;
}

interface TransactionCount {
  buy: number;
  sell: number;
  total: number;
}

interface VolumeAmount {
  buy: string;
  sell: string;
  total: string;
}

interface MakerCount {
  buy: number;
  sell: number;
  total: number;
}
```

### Enum 타입

```typescript
// Volume Range Filter
type VolumeRange = "small" | "medium" | "large";

// Volume Range Values
const VolumeRangeValues = {
  small: { min: 0, max: 1000 },       // $0 ~ $1,000
  medium: { min: 1000, max: 10000 },  // $1,000 ~ $10,000
  large: { min: 10000, max: null }    // $10,000+
};

// Chart Resolution
type ChartResolution =
  | "1" | "5" | "15" | "30"    // 분
  | "60" | "1H"                // 1시간
  | "240" | "4H"               // 4시간
  | "D" | "1D"                 // 1일
  | "W" | "1W"                 // 1주
  | "M" | "1M";                // 1개월

// Chart Type
type ChartType = "price" | "price_usd" | "market_cap" | "market_cap_usd";

// TimeFrame
type TimeFrame = "1" | "5" | "15" | "30" | "60" | "240" | "1D";

// Market Type
type MarketType = "CURVE" | "DEX";

// Swap Type
type SwapType = "BUY" | "SELL";

// Trade Type Filter
type TradeType = "BUY" | "SELL" | "ALL";
```
