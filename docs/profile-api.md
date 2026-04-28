# Profile API 문서

## 개요

Profile API는 사용자 프로필 및 활동 정보를 조회하기 위한 API입니다.

- **프로필 조회**: 계정 기본 정보 (닉네임, 바이오, 프로필 이미지)
- **보유 토큰 조회**: 사용자가 보유한 토큰 목록 및 잔액 정보
- **생성 토큰 조회**: 사용자가 생성한 토큰 목록
- **스왑 히스토리 조회**: 사용자의 거래 내역
- **포인트 히스토리 조회**: 사용자의 포인트 적립 내역 (인증 필요)

---

## API 엔드포인트

### 1. 프로필 조회 (`GET /profile/{account_id}`)

사용자의 기본 프로필 정보를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `account_id` | string | O | 사용자 Ethereum 주소 (EVM 형식) |

#### 응답
```json
{
  "account_info": {
    "account_id": "0x1234567890abcdef...",
    "nickname": "CryptoTrader",
    "bio": "DeFi enthusiast",
    "image_uri": "https://storage.nadapp.net/profiles/uuid.png"
  }
}
```

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 account_id)
- `404`: 사용자를 찾을 수 없음
- `500`: 내부 서버 에러

---

### 2. 보유 토큰 조회 (`GET /profile/hold-token/{account_id}`)

사용자가 보유한 토큰 목록과 잔액 정보를 페이지네이션으로 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `account_id` | string | O | 사용자 Ethereum 주소 (EVM 형식) |

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 (1부터 시작) |
| `limit` | integer | 10 | 페이지당 항목 수 (최대 100) |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |

#### 응답
```json
{
  "tokens": [
    {
      "token_info": {
        "token_id": "0x...",
        "name": "Token Name",
        "symbol": "TKN",
        "image_uri": "https://...",
        "description": "Token description",
        "is_graduated": false,
        "is_nsfw": false,
        "twitter": "https://x.com/...",
        "telegram": "https://t.me/...",
        "website": "https://...",
        "created_at": 1234567890,
        "creator": {
          "account_id": "0x...",
          "nickname": "Creator",
          "bio": "...",
          "image_uri": "https://..."
        },
        "is_cto": false,
        "version": "V1"
      },
      "balance_info": {
        "balance": "1000000000000000000",
        "token_price": "0.001",
        "native_price": "3000",
        "created_at": 1234567890
      },
      "market_info": {
        "market_type": "CURVE",
        "token_id": "0x...",
        "quote_info": {
          "quote_id": "0x5a4E0bFDeF88C9032CB4d24338C5EB3d3870BfDd",
          "name": "MONAD",
          "symbol": "MON",
          "decimals": 18,
          "image_uri": "https://storage.nadapp.net/quote/mon.webp"
        },
        "market_id": "0x...",
        "reserve_native": "100",
        "reserve_quote": "100",
        "reserve_token": "500000000",
        "token_price": "0.001",
        "native_price": "3000",
        "quote_price": "3000",
        "price": "0.000001",
        "price_usd": "0.003",
        "price_native": "0.000001",
        "price_quote": "0.000001",
        "total_supply": "1000000000",
        "volume": "50000",
        "ath_price": "0.000002",
        "ath_price_usd": "0.006",
        "ath_price_native": "0.000002",
        "ath_price_quote": "0.000002",
        "holder_count": 150,
        "fee_info": null
      }
    }
  ],
  "total_count": 25
}
```

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 account_id)
- `404`: 계정을 찾을 수 없음
- `500`: 내부 서버 에러

---

### 3. 생성 토큰 조회 (`GET /profile/tokens/created/{account_id}`)

사용자가 생성한 토큰 목록을 페이지네이션으로 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `account_id` | string | O | 사용자 Ethereum 주소 (EVM 형식) |

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 (1부터 시작) |
| `limit` | integer | 10 | 페이지당 항목 수 (최대 100) |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |

#### 응답
```json
{
  "tokens": [
    {
      "token_info": {
        "token_id": "0x...",
        "name": "My Token",
        "symbol": "MTK",
        "image_uri": "https://...",
        "description": "A token I created",
        "is_graduated": true,
        "is_nsfw": false,
        "twitter": "https://x.com/...",
        "telegram": "https://t.me/...",
        "website": "https://...",
        "created_at": 1234567890,
        "creator": {
          "account_id": "0x...",
          "nickname": "Me",
          "bio": "...",
          "image_uri": "https://..."
        },
        "is_cto": false,
        "version": "V1"
      },
      "market_info": {
        "market_type": "DEX",
        "token_id": "0x...",
        "market_id": "0x...",
        "reserve_native": "500",
        "reserve_token": "1000000000",
        "token_price": "0.005",
        "native_price": "3000",
        "price": "0.000005",
        "price_usd": "0.015",
        "price_native": "0.000005",
        "total_supply": "1000000000",
        "volume": "150000",
        "ath_price": "0.00001",
        "ath_price_usd": "0.03",
        "ath_price_native": "0.00001",
        "holder_count": 500
      },
      "balance_info": {
        "balance": "500000000000000000000",
        "token_price": "0.005",
        "native_price": "3000",
        "created_at": 1234567890
      },
      "reward_info": {
        "amount": "1000000000000000000",
        "claimed_amount": "500000000000000000",
        "proof": ["0xabc...", "0xdef..."],
        "claimable": true
      }
    }
  ],
  "total_count": 5
}
```

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 account_id)
- `404`: 계정을 찾을 수 없음
- `500`: 내부 서버 에러

---

### 4. Gift Fee 토큰 조회 (`GET /profile/gift-fee/{account_id}`)

특정 계정이 V2 gift vault의 receiver로 등록된 토큰 목록을 페이지네이션으로 조회합니다.
응답 항목은 `tokens/created` 와 동일한 `TokenCreatedInfo` 형식이라 UI에서 동일 카드 레이아웃 재사용 가능.

데이터 소스: `v2_gift_vault_stats` (`receiver = $account_id`).

#### 요청
- **Method**: `GET`
- **인증**: 불필요
- **캐시**: 30초 (env `GET_GIFT_FEE_RESPONSE_EXPIRATION` 으로 조정, ms 단위)

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `account_id` | string | O | 사용자 Ethereum 주소 (EVM 형식, EIP-55) |

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 (1부터 시작) |
| `limit` | integer | 10 | 페이지당 항목 수 (최대 100) |
| `direction` | string | DESC | 정렬 방향 (현재 무시 — `current_balance DESC, updated_at DESC` 고정) |

#### 응답

```json
{
  "tokens": [
    {
      "token_info": {
        "token_id": "0x350035555E10d9AfAF1566AaebfCeD5BA6C27777",
        "name": "Cosmos",
        "symbol": "ATOM",
        "image_uri": "https://storage.nadapp.net/...",
        "description": "...",
        "is_graduated": false,
        "is_nsfw": false,
        "twitter": "https://x.com/...",
        "telegram": null,
        "website": null,
        "created_at": 1714560000,
        "creator": {
          "account_id": "0x...",
          "nickname": "Creator",
          "bio": "...",
          "image_uri": "https://..."
        },
        "is_cto": false,
        "version": "V2"
      },
      "market_info": {
        "market_type": "V2_CURVE",
        "token_id": "0x350035555E10d9AfAF1566AaebfCeD5BA6C27777",
        "quote_info": {
          "quote_id": "0x5a4E0bFDeF88C9032CB4d24338C5EB3d3870BfDd",
          "name": "MONAD",
          "symbol": "MON",
          "decimals": 18,
          "image_uri": "https://storage.nadapp.net/quote/mon.webp"
        },
        "market_id": "0x...",
        "reserve_native": "100000000000000000000000",
        "reserve_quote": "100000000000000000000000",
        "reserve_token": "500000000000000000000000000",
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
        "holder_count": 150,
        "fee_info": {
          "creator_protocol_fee_rate": 100,
          "curve_protocol_fee_rate": 100,
          "dex_protocol_fee_rate": 50
        }
      },
      "balance_info": {
        "balance": "0",
        "token_price": "0.001",
        "native_price": "3000",
        "created_at": 0
      },
      "reward_info": {
        "amount": "0",
        "claimed_amount": "0",
        "proof": [],
        "claimable": false
      }
    }
  ],
  "total_count": 3
}
```

`tokens` 배열은 gift vault `current_balance` (받을 수 있는 잔액) 내림차순 정렬 — 받을 게 많은 토큰이 위로. 잔액이 같은 경우 `updated_at` 내림차순(최신 활동 우선)으로 보조 정렬.

#### 에러 응답
- `400`: 잘못된 account_id
- `500`: 내부 서버 에러

---

### 5. 스왑 히스토리 조회 (`GET /profile/swap-history/{account_id}`)

사용자의 거래(스왑) 내역을 페이지네이션으로 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `account_id` | string | O | 사용자 Ethereum 주소 (EVM 형식) |

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 (1부터 시작) |
| `limit` | integer | 10 | 페이지당 항목 수 (최대 100) |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |

#### 응답
```json
{
  "swaps": [
    {
      "token_info": {
        "token_id": "0x...",
        "name": "Token Name",
        "symbol": "TKN",
        "image_uri": "https://...",
        "description": "...",
        "is_graduated": false,
        "is_nsfw": false,
        "twitter": "https://x.com/...",
        "telegram": null,
        "website": null,
        "created_at": 1234567890,
        "creator": {
          "account_id": "0x...",
          "nickname": "Creator",
          "bio": "...",
          "image_uri": "https://..."
        },
        "is_cto": false,
        "version": "V1"
      },
      "swap_info": {
        "event_type": "BUY",
        "native_amount": "1000000000000000000",
        "token_amount": "1000000000000000000000",
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
- `400`: 잘못된 요청 (유효하지 않은 account_id)
- `500`: 내부 서버 에러

---

### 6. 포인트 히스토리 조회 (`GET /profile/point-history`)

사용자의 포인트 적립 내역을 조회합니다. **인증 필요**

#### 요청
- **Method**: `GET`
- **인증**: 필수 (Session Cookie)

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 (1부터 시작) |
| `limit` | integer | 10 | 페이지당 항목 수 (최대 100) |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |

#### 응답
```json
{
  "histories": [
    {
      "total_point": "15000",
      "history": [
        {
          "epoch": 1,
          "activity_type": "TRADE",
          "amount": "5000",
          "created_at": 1234567890
        },
        {
          "epoch": 1,
          "activity_type": "REFERRAL",
          "amount": "10000",
          "created_at": 1234567800
        }
      ],
      "created_at": 1234567890
    }
  ],
  "total_count": 10
}
```

#### 에러 응답
- `400`: 잘못된 요청
- `401`: 인증 실패 (세션 쿠키 없음 또는 만료)
- `500`: 내부 서버 에러

---

## TypeScript Interfaces

### 공통 타입

```typescript
// 페이지네이션 파라미터
interface PaginationParams {
  page?: number;       // default: 1, min: 1
  limit?: number;      // default: 10, max: 100
  direction?: string;  // "ASC" | "DESC", default: "DESC"
}

// 계정 정보
interface AccountInfo {
  account_id: string;
  nickname: string;
  bio: string;
  image_uri: string;
}

// 토큰 정보
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
  version: TokenVersion;
}

// 토큰 버전
type TokenVersion = "V1" | "V2";

// 마켓 타입
type MarketType = "CURVE" | "DEX" | "V2_CURVE" | "V2_DEX";

// 마켓 정보
interface QuoteInfo {
  quote_id: string;
  name: string;
  symbol: string;
  decimals: number;
  image_uri: string;
}

interface FeeInfo {
  creator_protocol_fee_rate: number;
  curve_protocol_fee_rate: number;
  dex_protocol_fee_rate: number;
}

interface MarketInfo {
  market_type: MarketType;
  token_id: string;
  quote_info: QuoteInfo;
  market_id: string;
  reserve_native: string;
  reserve_quote: string;
  reserve_token: string;
  token_price: string;      // Token/USD price
  native_price: string;     // MON/USD price
  quote_price: string;      // Quote/USD price
  price: string;            // MON/Token price
  price_usd: string;        // USD/Token price
  price_native: string;     // MON/Token price
  price_quote: string;      // Quote/Token price
  total_supply: string;
  volume: string;
  ath_price: string;
  ath_price_usd: string;
  ath_price_native: string;
  ath_price_quote: string;
  holder_count: number;
  fee_info: FeeInfo | null;  // V2 토큰만, V1은 null
}

// 잔액 정보
interface BalanceInfo {
  balance: string;          // 토큰 잔액 (wei)
  token_price: string;      // Token/USD price
  native_price: string;     // MON/USD price
  created_at: number;       // 보유 시작 시점
}

// 스왑 타입
type SwapType = "BUY" | "SELL";

// 스왑 정보
interface SwapInfo {
  event_type: SwapType;
  native_amount: string;    // Native 토큰 수량 (wei)
  token_amount: string;     // 토큰 수량 (wei)
  native_price: string;     // 실행 시점 Native/USD 가격
  value: string;            // 실행 시점 USD 가치
  transaction_hash: string;
  created_at: number;
}

// 리워드 정보
interface RewardInfo {
  amount: string;           // 총 리워드 수량
  claimed_amount: string;   // 청구한 리워드 수량
  proof: string[];          // Merkle proof
  claimable: boolean;       // 청구 가능 여부
}
```

### 응답 타입

```typescript
// GET /profile/{account_id}
interface ProfileResponse {
  account_info: AccountInfo;
}

// GET /profile/hold-token/{account_id}
interface HoldTokenResponse {
  tokens: TokenWithBalanceInfo[];
  total_count: number;
}

interface TokenWithBalanceInfo {
  token_info: TokenInfo;
  balance_info: BalanceInfo;
  market_info: MarketInfo;
}

// GET /profile/tokens/created/{account_id}
interface CreatedTokensResponse {
  tokens: TokenCreatedInfo[];
  total_count: number;
}

interface TokenCreatedInfo {
  token_info: TokenInfo;
  market_info: MarketInfo;
  balance_info: BalanceInfo;
  reward_info: RewardInfo;
}

// GET /profile/swap-history/{account_id}
interface SwapHistoryResponse {
  swaps: TokenSwapInfo[];
  total_count: number;
}

interface TokenSwapInfo {
  token_info: TokenInfo;
  swap_info: SwapInfo;
}

// GET /profile/point-history
interface PointHistoryResponse {
  histories: PointRecordTotal[];
  total_count: number;
}

interface PointRecordTotal {
  total_point: string;
  history: PointRecord[];
  created_at: number;
}

interface PointRecord {
  epoch: number;
  activity_type: string;
  amount: string;
  created_at: number;
}
```

---

## 에러 응답 형식

모든 API는 에러 발생 시 다음 형식으로 응답합니다:

```json
{
  "error": "Error message description"
}
```

| 상태 코드 | 설명 |
|-----------|------|
| `400` | 잘못된 요청 (유효하지 않은 파라미터) |
| `401` | 인증 실패 (세션 쿠키 없음 또는 만료) |
| `404` | 리소스를 찾을 수 없음 |
| `500` | 내부 서버 에러 |
