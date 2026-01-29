# Trend API 문서

## 개요

Trend API는 트렌드 토큰의 조회 및 관리를 위한 API입니다.

- **트렌드 토큰 조회**: 등록된 트렌드 토큰 목록을 24시간 가격 변동률과 함께 반환
- **트렌드 토큰 등록**: Admin 전용 - 새로운 트렌드 토큰을 등록

---

## API 엔드포인트

### 1. 트렌드 토큰 조회 (`GET /trend`)

등록된 모든 트렌드 토큰을 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### 캐싱
- Redis 캐시 적용 (Single Flight 패턴으로 동시 요청 처리)
- 캐시 키: `trend:service:all`

#### 응답
```json
{
  "tokens": [
    {
      "token_info": {
        "token_id": "0x1234567890abcdef...",
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
          "bio": "Creator bio",
          "image_uri": "https://..."
        },
        "is_cto": false,
        "hackathon_info": null
      },
      "market_info": {
        "market_type": "CURVE",
        "token_id": "0x...",
        "market_id": "0x...",
        "reserve_native": "100",
        "reserve_token": "500000000",
        "token_price": "0.001",
        "native_price": "3000",
        "price": "0.000001",
        "price_usd": "0.003",
        "price_native": "0.000001",
        "total_supply": "1000000000",
        "volume": "50000",
        "ath_price": "0.000002",
        "ath_price_usd": "0.006",
        "ath_price_native": "0.000002",
        "holder_count": 150
      },
      "percent": 15.5
    }
  ]
}
```

#### 응답 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `tokens` | TrendToken[] | 트렌드 토큰 목록 |
| `tokens[].token_info` | TokenInfo | 토큰 기본 정보 |
| `tokens[].market_info` | MarketInfo | 마켓 정보 (가격, 거래량 등) |
| `tokens[].percent` | number | 24시간 가격 변동률 (%) |

#### 에러 응답
- `500`: 내부 서버 에러

---

### 2. 트렌드 토큰 등록 (`POST /cms/trend/insert`)

새로운 트렌드 토큰을 등록합니다. **Admin 권한 필요**

#### 요청
- **Method**: `POST`
- **Content-Type**: `application/json`
- **인증**: 필수 (Admin only)

#### Request Body
```json
{
  "token_ids": [
    "0x1234567890abcdef...",
    "0x5678901234abcdef...",
    "0xabcdef1234567890..."
  ]
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `token_ids` | string[] | O | 등록할 토큰 ID 배열 (최대 50개) |

#### 유효성 검증

- `token_ids` 배열은 최대 50개까지 허용
- 각 `token_id`는 유효한 EVM 주소 형식이어야 함

#### 응답
```json
{
  "success": true
}
```

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 token_id, 최대 개수 초과)
- `401`: 인증 실패 (Admin 권한 없음)
- `500`: 내부 서버 에러

---

## TypeScript Interfaces

### 요청 타입

```typescript
// POST /cms/trend/insert
interface InsertTrendRequest {
  token_ids: string[];  // 최대 50개, EVM 주소 형식
}
```

### 응답 타입

```typescript
// GET /trend
interface TrendResponse {
  tokens: TrendToken[];
}

interface TrendToken {
  token_info: TokenInfo;
  market_info: MarketInfo;
  percent: number;  // 24h 가격 변동률
}

// POST /cms/trend/insert
interface CmsActionResponse {
  success: boolean;
}

// 공통 타입
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

interface AccountInfo {
  account_id: string;
  nickname: string;
  bio: string;
  image_uri: string;
}

interface MarketInfo {
  market_type: "CURVE" | "DEX";
  token_id: string;
  market_id: string;
  reserve_native: string;
  reserve_token: string;
  token_price: string;      // Token/USD price
  native_price: string;     // MON/USD price
  price: string;            // MON/Token price
  price_usd: string;        // USD/Token price
  price_native: string;     // MON/Token price
  total_supply: string;     // 총 공급량 (본딩 커브 마켓캡 계산용)
  volume: string;           // 총 거래량
  ath_price: string;        // ATH 가격 (USD)
  ath_price_usd: string;    // ATH 가격 (USD)
  ath_price_native: string; // ATH 가격 (Native)
  holder_count: number;     // 홀더 수
}

interface HackathonInfo {
  creator: HackathonCreatorInfo;
  project: HackathonProjectInfo;
}
```

---

## 캐싱 전략

### Redis 캐싱
- 트렌드 토큰 조회 시 Redis 캐시를 먼저 확인
- 캐시 미스 시 DB 조회 후 Redis에 결과 저장

### Single Flight 패턴
- 동일한 요청에 대한 동시 호출을 방지
- 캐시 키: `trend:service:all`
- 트렌드 토큰 등록 시 캐시 자동 무효화

---

## Admin 권한

### 트렌드 토큰 등록 권한
- `POST /cms/trend/insert` 엔드포인트는 Admin 권한이 필요
- 세션 주소로 Admin 여부 확인
- Admin이 아닌 경우 `401 Unauthorized` 반환
