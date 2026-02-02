# Agent API 문서

## 개요

AI 에이전트 및 외부 서비스를 위한 통합 API입니다. Trading 데이터, Token 정보, Holdings, Token 생성 기능을 제공합니다.

### 인증

모든 Agent API는 `X-API-Key` 헤더가 필요합니다.

```bash
curl https://api.nad.fun/agent/token/0x1234... \
  -H "X-API-Key: nadfun_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
```

> API Key 발급: [api-key.md](./api-key.md) 참조

### Rate Limit

- **60 req/min** per API Key
- 초과 시 `429 Too Many Requests` 반환

---

## Trading Data Endpoints

### 1. Chart Data (`GET /agent/chart/:token_id`)

토큰의 가격 차트 데이터(OHLCV)를 조회합니다.

#### 요청

```bash
curl "https://api.nad.fun/agent/chart/0x1234...?resolution=60&from=1704067200&to=1704153600" \
  -H "X-API-Key: nadfun_xxx"
```

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `resolution` | string | O | 차트 해상도: `1`, `5`, `15`, `30`, `60`, `240`, `1D` |
| `from` | i64 | O | 시작 타임스탬프 (Unix seconds) |
| `to` | i64 | O | 종료 타임스탬프 (Unix seconds) |
| `countback` | i32 | X | 최대 캔들 수 (기본: 500, 최대: 3000) |
| `chart_type` | string | X | `price` (기본), `price_usd`, `market_cap`, `market_cap_usd` |

#### 응답: `BarResponse`

```json
{
  "k": "price",
  "t": [1704067200, 1704070800],
  "o": ["0.001", "0.0012"],
  "h": ["0.0015", "0.0014"],
  "l": ["0.0009", "0.0011"],
  "c": ["0.0012", "0.0013"],
  "v": ["1000000", "1500000"],
  "s": "ok"
}
```

| 필드 | 타입 | 설명 |
|------|------|------|
| `k` | string | 차트 타입 |
| `t` | i64[] | 타임스탬프 배열 (초 단위) |
| `o` | string[] | 시가 배열 |
| `h` | string[] | 고가 배열 |
| `l` | string[] | 저가 배열 |
| `c` | string[] | 종가 배열 |
| `v` | string[] | 거래량 배열 |
| `s` | string | 상태: `ok`, `error`, `no_data` |

---

### 2. Swap History (`GET /agent/swap-history/:token_id`)

토큰의 스왑/거래 내역을 조회합니다.

#### 요청

```bash
curl "https://api.nad.fun/agent/swap-history/0x1234...?page=1&limit=20&trade_type=BUY" \
  -H "X-API-Key: nadfun_xxx"
```

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `page` | i64 | X | 페이지 번호 (기본: 1) |
| `limit` | i64 | X | 페이지당 항목 수 (기본: 10) |
| `direction` | string | X | `ASC` 또는 `DESC` |
| `trade_type` | string | X | `BUY`, `SELL`, `ALL` (기본: ALL) |
| `volume_ranges` | string | X | 볼륨 필터: `small`, `medium`, `large` (쉼표 구분) |
| `account_id` | string | X | 특정 계정의 거래만 필터 |

#### 응답: `TokenSwapResponse`

```json
{
  "swaps": [
    {
      "account_info": {
        "account_id": "0xabc...",
        "nickname": "trader123",
        "bio": "",
        "image_uri": "https://..."
      },
      "swap_info": {
        "event_type": "BUY",
        "native_amount": "1000000000000000000",
        "token_amount": "5000000000000000000000",
        "native_price": "3.0",
        "value": "3.0",
        "transaction_hash": "0xdef...",
        "created_at": 1704067200
      }
    }
  ],
  "total_count": 150
}
```

---

### 3. Market Data (`GET /agent/market/:token_id`)

토큰의 현재 시장 데이터를 조회합니다.

#### 요청

```bash
curl "https://api.nad.fun/agent/market/0x1234..." \
  -H "X-API-Key: nadfun_xxx"
```

#### 응답: `MarketResponse`

```json
{
  "market_info": {
    "market_type": "CURVE",
    "token_id": "0x1234...",
    "market_id": "0x5678...",
    "reserve_native": "10000000000000000000000",
    "reserve_token": "500000000000000000000000000",
    "token_price": "0.00002",
    "native_price": "3.0",
    "price": "0.00002",
    "price_usd": "0.00006",
    "price_native": "0.00002",
    "total_supply": "1000000000000000000000000000",
    "volume": "50000000000000000000000",
    "ath_price": "0.00005",
    "ath_price_usd": "0.00015",
    "ath_price_native": "0.00005",
    "holder_count": 1234
  }
}
```

| 필드 | 설명 |
|------|------|
| `market_type` | `CURVE` 또는 `DEX` |
| `reserve_native` | 네이티브 토큰(MON) 리저브 |
| `reserve_token` | 토큰 리저브 |
| `price` | MON/Token 가격 |
| `price_usd` | USD/Token 가격 |
| `ath_price_usd` | ATH 가격 (USD) |

---

### 4. Trading Metrics (`GET /agent/metrics/:token_id`)

토큰의 거래 메트릭을 조회합니다.

#### 요청

```bash
curl "https://api.nad.fun/agent/metrics/0x1234...?timeframes=1,5,60,1D" \
  -H "X-API-Key: nadfun_xxx"
```

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `timeframes` | string | O | 쉼표로 구분: `1`, `5`, `15`, `30`, `60`, `240`, `1D` |

#### 응답: `MetricsBatchResponse`

```json
{
  "metrics": [
    {
      "timeframe": "1m",
      "percent": 2.5,
      "transactions": {
        "buy": 15,
        "sell": 10,
        "total": 25
      },
      "volume": {
        "buy": "1000000000000000000",
        "sell": "500000000000000000",
        "total": "1500000000000000000"
      },
      "makers": {
        "buy": 12,
        "sell": 8,
        "total": 20
      }
    }
  ]
}
```

---

## Token Data Endpoint

### 5. Token Info (`GET /agent/token/:token_id`)

토큰의 전체 정보를 조회합니다.

#### 요청

```bash
curl "https://api.nad.fun/agent/token/0x1234..." \
  -H "X-API-Key: nadfun_xxx"
```

#### 응답: `TokenResponse`

```json
{
  "token_info": {
    "token_id": "0x1234...",
    "name": "My Token",
    "symbol": "MTK",
    "image_uri": "https://storage.nadapp.net/images/abc.png",
    "description": "A great token",
    "is_graduated": false,
    "is_nsfw": false,
    "twitter": "https://x.com/mytoken",
    "telegram": "https://t.me/mytoken",
    "website": "https://mytoken.com",
    "created_at": 1704067200,
    "creator": {
      "account_id": "0xabc...",
      "nickname": "creator",
      "bio": "",
      "image_uri": "https://..."
    },
    "is_cto": false,
    "hackathon_info": null
  }
}
```

---

## Holdings Endpoint

### 6. User Holdings (`GET /agent/holdings/:account_id`)

사용자의 토큰 보유 현황을 조회합니다.

#### 요청

```bash
curl "https://api.nad.fun/agent/holdings/0xabc...?page=1&limit=20" \
  -H "X-API-Key: nadfun_xxx"
```

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `page` | i64 | X | 페이지 번호 |
| `limit` | i64 | X | 페이지당 항목 수 |

#### 응답: `HoldTokenResponse`

```json
{
  "tokens": [
    {
      "token_info": {
        "token_id": "0x1234...",
        "name": "My Token",
        "symbol": "MTK",
        "image_uri": "https://...",
        "description": "...",
        "is_graduated": false,
        "is_nsfw": false,
        "created_at": 1704067200,
        "creator": { ... },
        "is_cto": false
      },
      "balance_info": {
        "balance": "5000000000000000000000",
        "token_price": "0.00006",
        "native_price": "3.0",
        "created_at": 1704100000
      },
      "market_info": {
        "market_type": "CURVE",
        "price_usd": "0.00006",
        ...
      }
    }
  ],
  "total_count": 5
}
```

---

## Token Creation Endpoints

### 7. Upload Image (`POST /agent/token/image`)

토큰 이미지를 업로드합니다.

#### 요청

```bash
curl -X POST "https://api.nad.fun/agent/token/image" \
  -H "X-API-Key: nadfun_xxx" \
  -H "Content-Type: image/png" \
  --data-binary @token_image.png
```

#### 제한사항

- 최대 파일 크기: **5MB**
- 지원 형식: PNG, JPG, GIF, WEBP, SVG

#### 응답: `UploadImageResponse`

```json
{
  "is_nsfw": false,
  "image_uri": "https://storage.nadapp.net/images/abc123.png"
}
```

---

### 8. Upload Metadata (`POST /agent/token/metadata`)

토큰 메타데이터를 업로드합니다.

#### 요청

```bash
curl -X POST "https://api.nad.fun/agent/token/metadata" \
  -H "X-API-Key: nadfun_xxx" \
  -H "Content-Type: application/json" \
  -d '{
    "image_uri": "https://storage.nadapp.net/images/abc123.png",
    "name": "My Token",
    "symbol": "MTK",
    "description": "A great token",
    "website": "https://mytoken.com",
    "twitter": "https://x.com/mytoken",
    "telegram": "https://t.me/mytoken"
  }'
```

#### Request Body: `UploadMetadataRequest`

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `image_uri` | string | O | 이미지 URL (upload_image로 업로드한 URL) |
| `name` | string | O | 토큰 이름 (1-32자) |
| `symbol` | string | O | 토큰 심볼 (1-10자, 영문숫자만) |
| `description` | string | X | 토큰 설명 (최대 500자) |
| `website` | string | X | 웹사이트 URL (`https://`로 시작) |
| `twitter` | string | X | Twitter/X URL (`https://x.com/`으로 시작) |
| `telegram` | string | X | 텔레그램 URL (`https://t.me/`으로 시작) |

#### 응답: `UploadMetadataResponse`

```json
{
  "metadata_uri": "https://storage.nadapp.net/metadata/def456.json",
  "metadata": {
    "name": "My Token",
    "symbol": "MTK",
    "description": "A great token",
    "image_uri": "https://storage.nadapp.net/images/abc123.png",
    "website": "https://mytoken.com",
    "twitter": "https://x.com/mytoken",
    "telegram": "https://t.me/mytoken",
    "is_nsfw": false
  }
}
```

---

### 9. Created Tokens (`GET /agent/token/created/:account_id`)

계정이 생성한 토큰 목록과 리워드 정보를 조회합니다.

#### 요청

```bash
curl "https://api.nad.fun/agent/token/created/0xabc...?page=1&limit=10" \
  -H "X-API-Key: nadfun_xxx"
```

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `page` | i64 | X | 페이지 번호 |
| `limit` | i64 | X | 페이지당 항목 수 |

#### 응답: `CreatedTokensResponse`

```json
{
  "tokens": [
    {
      "token_info": {
        "token_id": "0x1234...",
        "name": "My Token",
        "symbol": "MTK",
        ...
      },
      "market_info": {
        "price_usd": "0.00006",
        "holder_count": 150,
        ...
      },
      "balance_info": {
        "balance": "100000000000000000000000",
        "token_price": "0.00006",
        "native_price": "3.0",
        "created_at": 1704067200
      },
      "reward_info": {
        "amount": "5000000000000000000",
        "claimed_amount": "1000000000000000000",
        "proof": ["0x...", "0x..."],
        "claimable": true
      }
    }
  ],
  "total_count": 3
}
```

---

### 10. Mine Salt (`POST /agent/salt`)

특정 suffix로 끝나는 토큰 주소를 생성하기 위한 salt 값을 마이닝합니다.

#### 요청

```bash
curl -X POST "https://api.nad.fun/agent/salt" \
  -H "X-API-Key: nadfun_xxx" \
  -H "Content-Type: application/json" \
  -d '{
    "creator": "0x742d35Cc6634C0532925a3b844Bc9e7595f70143",
    "name": "My Token",
    "symbol": "MTK",
    "metadata_uri": "https://storage.nadapp.net/metadata/abc123.json"
  }'
```

#### Request Body: `MineSaltRequest`

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `creator` | string | O | 토큰 생성자 주소 (0x + 40 hex) |
| `name` | string | O | 토큰 이름 (1-32자) |
| `symbol` | string | O | 토큰 심볼 (1-10자, 영문숫자만) |
| `metadata_uri` | string | O | 메타데이터 URI (허용된 도메인 필수) |

#### 응답: `MineSaltResponse`

```json
{
  "salt": "0x000000000000000000000000000000000000000000000000000000000000a3f5",
  "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f7777"
}
```

| 필드 | 타입 | 설명 |
|------|------|------|
| `salt` | string | 원하는 주소를 생성하는 salt 값 (0x + 64 hex) |
| `address` | string | 계산된 토큰 주소 (suffix로 끝남) |

#### 에러 응답

- `400 Bad Request`: 파라미터 검증 실패
- `408 Request Timeout`: 최대 반복 횟수 도달 (주소를 찾지 못함)

---

## TypeScript Interfaces

```typescript
// ==================== Common ====================

interface AccountInfo {
  account_id: string;
  nickname: string;
  bio: string;
  image_uri: string;
}

interface TokenInfo {
  token_id: string;
  name: string;
  symbol: string;
  image_uri: string;
  description?: string;
  is_graduated: boolean;
  is_nsfw: boolean;
  twitter?: string;
  telegram?: string;
  website?: string;
  created_at: number;
  creator: AccountInfo;
  is_cto: boolean;
  hackathon_info?: HackathonInfo;
}

interface MarketInfo {
  market_type: "CURVE" | "DEX";
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

interface SwapInfo {
  event_type: "BUY" | "SELL";
  native_amount: string;
  token_amount: string;
  native_price: string;
  value: string;
  transaction_hash: string;
  created_at: number;
}

interface BalanceInfo {
  balance: string;
  token_price: string;
  native_price: string;
  created_at: number;
}

interface RewardInfo {
  amount: string;
  claimed_amount: string;
  proof: string[];
  claimable: boolean;
}

// ==================== Responses ====================

// GET /agent/chart/:token_id
interface BarResponse {
  k: string;
  t: number[];
  o: string[];
  h: string[];
  l: string[];
  c: string[];
  v: string[];
  s: string;
}

// GET /agent/swap-history/:token_id
interface TokenSwapResponse {
  swaps: Array<{
    account_info: AccountInfo;
    swap_info: SwapInfo;
  }>;
  total_count: number;
}

// GET /agent/market/:token_id
interface MarketResponse {
  market_info: MarketInfo;
}

// GET /agent/metrics/:token_id
interface MetricsBatchResponse {
  metrics: Array<{
    timeframe: string;
    percent: number;
    transactions: { buy: number; sell: number; total: number };
    volume: { buy: string; sell: string; total: string };
    makers: { buy: number; sell: number; total: number };
  }>;
}

// GET /agent/token/:token_id
interface TokenResponse {
  token_info: TokenInfo;
}

// GET /agent/holdings/:account_id
interface HoldTokenResponse {
  tokens: Array<{
    token_info: TokenInfo;
    balance_info: BalanceInfo;
    market_info: MarketInfo;
  }>;
  total_count: number;
}

// POST /agent/token/image
interface UploadImageResponse {
  is_nsfw: boolean;
  image_uri: string;
}

// POST /agent/token/metadata
interface UploadMetadataRequest {
  image_uri: string;
  name: string;
  symbol: string;
  description?: string;
  website?: string;
  twitter?: string;
  telegram?: string;
}

interface UploadMetadataResponse {
  metadata_uri: string;
  metadata: {
    name: string;
    symbol: string;
    description?: string;
    image_uri: string;
    website?: string;
    twitter?: string;
    telegram?: string;
    is_nsfw: boolean;
  };
}

// GET /agent/token/created/:account_id
interface CreatedTokensResponse {
  tokens: Array<{
    token_info: TokenInfo;
    market_info: MarketInfo;
    balance_info: BalanceInfo;
    reward_info: RewardInfo;
  }>;
  total_count: number;
}

// POST /agent/salt
interface MineSaltRequest {
  creator: string;
  name: string;
  symbol: string;
  metadata_uri: string;
}

interface MineSaltResponse {
  salt: string;
  address: string;
}
```

---

## 에러 코드

| 상태 코드 | 설명 |
|-----------|------|
| 200 | 성공 |
| 400 | 잘못된 요청 (유효하지 않은 token_id, account_id, 파라미터) |
| 401 | 인증 실패 (API Key 필요 또는 유효하지 않음) |
| 404 | 토큰/계정을 찾을 수 없음 |
| 413 | 파일 크기 초과 (이미지 업로드) |
| 429 | Rate limit 초과 |
| 500 | 내부 서버 에러 |

---

## 사용 예시

### Python

```python
import requests

API_KEY = "nadfun_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
BASE_URL = "https://api.nad.fun"

headers = {"X-API-Key": API_KEY}

# Get token info
token_id = "0x1234567890abcdef..."
response = requests.get(f"{BASE_URL}/agent/token/{token_id}", headers=headers)
token_data = response.json()
print(f"Token: {token_data['token_info']['name']}")

# Get chart data
params = {
    "resolution": "60",
    "from": 1704067200,
    "to": 1704153600
}
response = requests.get(f"{BASE_URL}/agent/chart/{token_id}", headers=headers, params=params)
chart_data = response.json()

# Upload image
with open("token.png", "rb") as f:
    response = requests.post(
        f"{BASE_URL}/agent/token/image",
        headers={**headers, "Content-Type": "image/png"},
        data=f.read()
    )
image_data = response.json()
print(f"Image URI: {image_data['image_uri']}")
print(f"Is NSFW: {image_data['is_nsfw']}")
```

### JavaScript/TypeScript

```typescript
const API_KEY = "nadfun_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
const BASE_URL = "https://api.nad.fun";

const headers = { "X-API-Key": API_KEY };

// Get token info
const tokenId = "0x1234567890abcdef...";
const tokenRes = await fetch(`${BASE_URL}/agent/token/${tokenId}`, { headers });
const tokenData = await tokenRes.json();
console.log(`Token: ${tokenData.token_info.name}`);

// Get market data
const marketRes = await fetch(`${BASE_URL}/agent/market/${tokenId}`, { headers });
const marketData = await marketRes.json();
console.log(`Price USD: ${marketData.market_info.price_usd}`);

// Get user holdings
const accountId = "0xabc...";
const holdingsRes = await fetch(
  `${BASE_URL}/agent/holdings/${accountId}?page=1&limit=20`,
  { headers }
);
const holdings = await holdingsRes.json();
console.log(`Holding ${holdings.total_count} tokens`);
```
