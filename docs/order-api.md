# Order API 문서

## 개요

Order API는 토큰 목록을 다양한 기준으로 정렬하여 조회하는 API입니다.

- **생성 시간 순 조회**: 토큰 생성 시간 기준 정렬
- **마켓캡 순 조회**: 시가총액(price * reserve_token) 기준 정렬
- **최신 거래 순 조회**: 마지막 거래 시간 기준 정렬
- **해커톤 토큰 조회**: 해커톤 등록 토큰만 마켓캡 순으로 정렬

---

## API 엔드포인트

### 1. 생성 시간 순 조회 (`GET /order/creation_time`)

토큰 목록을 생성 시간(`created_at`) 기준으로 정렬하여 반환합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 |
| `limit` | integer | 20 | 페이지당 항목 수 |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |
| `is_nsfw` | boolean | false | NSFW 토큰 포함 여부 |

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
        "description": "...",
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
  ],
  "total_count": 100
}
```

#### 에러 응답
- `400`: 잘못된 페이지네이션 파라미터
- `500`: 내부 서버 에러

---

### 2. 마켓캡 순 조회 (`GET /order/market_cap`)

토큰 목록을 시가총액(price * reserve_token) 기준으로 정렬하여 반환합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 |
| `limit` | integer | 20 | 페이지당 항목 수 |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |
| `is_nsfw` | boolean | false | NSFW 토큰 포함 여부 |

#### 응답
응답 형식은 생성 시간 순 조회와 동일합니다.

#### 에러 응답
- `400`: 잘못된 페이지네이션 파라미터
- `500`: 내부 서버 에러

---

### 3. 최신 거래 순 조회 (`GET /order/latest_trade`)

토큰 목록을 마지막 거래 시간(`latest_trade_at`) 기준으로 정렬하여 반환합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 |
| `limit` | integer | 20 | 페이지당 항목 수 |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |
| `is_nsfw` | boolean | false | NSFW 토큰 포함 여부 |

#### 응답
응답 형식은 생성 시간 순 조회와 동일합니다.

#### 에러 응답
- `400`: 잘못된 페이지네이션 파라미터
- `500`: 내부 서버 에러

---

### 4. 해커톤 토큰 조회 (`GET /order/hackathon`)

해커톤에 등록된 토큰만 마켓캡 순으로 정렬하여 반환합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 |
| `limit` | integer | 20 | 페이지당 항목 수 |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |
| `is_nsfw` | boolean | false | NSFW 토큰 포함 여부 |

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
        "description": "...",
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
        "hackathon_info": {
          "creator": {
            "github_id": "MonkeyGyu",
            "image_uri": "https://avatars.githubusercontent.com/...",
            "name": "Gyu",
            "github_url": "https://github.com/MonkeyGyu",
            "follower_count": 100,
            "following_count": 50,
            "repo_count": 30,
            "star_count": 500,
            "bio": "Developer",
            "twitter": "@username",
            "discord": "username#1234",
            "telegram": "@username",
            "linkedin": "https://linkedin.com/in/username",
            "account_id": "0x..."
          },
          "project": {
            "github_url": "https://github.com/owner/repo",
            "name": "My Project",
            "description": "Project description",
            "keywords": ["defi", "nft", "trading"],
            "screenshot_uri": "https://storage.nadapp.net/...",
            "website": "https://myproject.com",
            "youtube": "https://youtube.com/...",
            "star_count": 150,
            "fork_count": 30,
            "topics": ["blockchain", "web3"],
            "language": "TypeScript"
          }
        }
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
  ],
  "total_count": 25
}
```

#### 특징
- 해커톤에 등록된 토큰만 필터링
- 마켓캡(price) 기준 정렬
- `hackathon_info` 필드에 개발자/프로젝트 정보 자동 포함
- 24시간 가격 변동률(`percent`) 포함

#### 에러 응답
- `400`: 잘못된 페이지네이션 파라미터
- `500`: 내부 서버 에러

---

## TypeScript Interfaces

### 요청 타입

```typescript
// GET /order/* Query Parameters
interface OrderQuery {
  page?: number;       // default: 1
  limit?: number;      // default: 20
  direction?: string;  // "ASC" | "DESC", default: "DESC"
  is_nsfw?: boolean;   // default: false
}
```

### 응답 타입

```typescript
// 공통 응답 타입
interface OrderTokenResponse {
  tokens: OrderToken[];
  total_count: number;
}

interface OrderToken {
  token_info: TokenInfo;
  market_info: MarketInfo;
  percent: number;  // 24h 가격 변동률 (%)
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
  reserve_native: string;       // Native 토큰(MON) 보유량
  reserve_token: string;        // 토큰 보유량
  token_price: string;          // Token/USD 가격
  native_price: string;         // MON/USD 가격
  price: string;                // MON/Token 가격
  price_usd: string;            // USD/Token 가격
  price_native: string;         // MON/Token 가격
  total_supply: string;         // 총 공급량
  volume: string;               // 총 거래량
  ath_price: string;            // ATH 가격 (USD)
  ath_price_usd: string;        // ATH 가격 (USD)
  ath_price_native: string;     // ATH 가격 (Native)
  holder_count: number;         // 홀더 수
}

interface HackathonInfo {
  creator: HackathonCreatorInfo;
  project: HackathonProjectInfo;
}

interface HackathonCreatorInfo {
  github_id: string;
  image_uri?: string;
  name?: string;
  github_url?: string;
  follower_count: number;
  following_count: number;
  repo_count: number;
  star_count: number;
  bio?: string;
  twitter: string;
  discord?: string;
  telegram?: string;
  linkedin?: string;
  account_id: string;
}

interface HackathonProjectInfo {
  github_url: string;
  name: string;
  description: string;
  keywords: string[];
  screenshot_uri: string;
  website?: string;
  youtube?: string;
  star_count: number;
  fork_count: number;
  topics?: string[];
  language?: string;
}
```

### 정렬 기준 타입

```typescript
// 내부 정렬 타입 (API에서는 URL path로 구분)
type TokenOrderType =
  | "market_cap"      // 시가총액 기준
  | "creation_time"   // 생성 시간 기준
  | "latest_trade"    // 최신 거래 기준
  | "hackathon";      // 해커톤 토큰 (마켓캡 순)
```

---

## 엔드포인트 요약

| 엔드포인트 | 설명 | 정렬 기준 |
|------------|------|-----------|
| `GET /order/creation_time` | 생성 시간 순 조회 | `created_at` |
| `GET /order/market_cap` | 마켓캡 순 조회 | `price * reserve_token` |
| `GET /order/latest_trade` | 최신 거래 순 조회 | `latest_trade_at` |
| `GET /order/hackathon` | 해커톤 토큰 조회 | 마켓캡 (해커톤 토큰만) |
