# Search API 문서

## 개요

Search API는 토큰과 계정을 동시에 검색하는 통합 검색 API입니다.

- **토큰 검색**: 토큰 이름, 심볼, 또는 주소로 검색
- **계정 검색**: 계정 주소 또는 닉네임으로 검색

검색 결과는 토큰과 계정 정보를 각각 분리하여 반환합니다.

---

## API 엔드포인트

### 1. 통합 검색 (`GET /search/:name`)

토큰과 계정을 동시에 검색합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `name` | string | O | 검색어 (토큰 이름, 심볼, 주소 또는 계정 주소/닉네임) |

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 (최소 1) |
| `limit` | integer | 10 | 페이지당 항목 수 (1~100) |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |

#### 검색어 제한

- 최소 1자 이상 (빈 문자열 불가)
- 최대 100자 이하
- `0x`만 단독으로 사용 불가

#### 응답

```json
{
  "account_result": {
    "accounts": [
      {
        "account_info": {
          "account_id": "0x1234567890abcdef...",
          "nickname": "User123",
          "bio": "Hello, I'm a trader",
          "image_uri": "https://storage.nadapp.net/profiles/uuid.png"
        }
      }
    ],
    "total_count": 5
  },
  "token_result": {
    "tokens": [
      {
        "token_info": {
          "token_id": "0xabcdef1234567890...",
          "name": "PUMP Token",
          "symbol": "PUMP",
          "image_uri": "https://storage.nadapp.net/tokens/uuid.png",
          "description": "A decentralized trading token",
          "is_graduated": false,
          "is_nsfw": false,
          "twitter": "https://x.com/pumptoken",
          "telegram": "https://t.me/pumptoken",
          "website": "https://pumptoken.io",
          "created_at": 1234567890,
          "creator": {
            "account_id": "0x...",
            "nickname": "Creator",
            "bio": "Token creator",
            "image_uri": "https://..."
          },
          "is_cto": false,
          "hackathon_info": null
        },
        "market_info": {
          "market_type": "CURVE",
          "token_id": "0xabcdef1234567890...",
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
        }
      }
    ],
    "total_count": 10
  }
}
```

#### 에러 응답

| 상태 코드 | 설명 |
|-----------|------|
| `400` | 잘못된 요청 (빈 검색어, 검색어 100자 초과, `0x` 단독 사용) |
| `500` | 내부 서버 에러 |

---

## TypeScript Interfaces

### 요청 타입

```typescript
// GET /search/:name Query Parameters
interface SearchQuery {
  page?: number;       // default: 1, min: 1
  limit?: number;      // default: 10, min: 1, max: 100
  direction?: string;  // "ASC" | "DESC", default: "DESC"
}
```

### 응답 타입

```typescript
// GET /search/:name 응답
interface SearchResponse {
  account_result: AccountSearchResponse;
  token_result: TokenSearchResponse;
}

interface AccountSearchResponse {
  accounts: AccountSearchResult[];
  total_count: number;
}

interface AccountSearchResult {
  account_info: AccountInfo;
}

interface TokenSearchResponse {
  tokens: TokenSearchResult[];
  total_count: number;
}

interface TokenSearchResult {
  token_info: TokenInfo;
  market_info: MarketInfo;
}

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
  market_type: "CURVE" | "DEX" | "V2_CURVE" | "V2_DEX";
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
  ath_price: string;        // ATH price (USD)
  ath_price_usd: string;    // ATH price (USD)
  ath_price_native: string; // ATH price (Native)
  ath_price_quote: string;  // ATH price (Quote)
  holder_count: number;
  fee_info: FeeInfo | null;  // V2 토큰만, V1은 null
}

interface HackathonInfo {
  team: HackathonTeamInfo;
  project: HackathonProjectInfo;
}

interface HackathonTeamInfo {
  id: string;
  name: string;
  members: HackathonTeamMemberInfo[];
}

interface HackathonTeamMemberInfo {
  email: string;
  discord?: string;
  twitter?: string;
  linkedin?: string;
  github?: HackathonMemberGitHubInfo;
}

interface HackathonMemberGitHubInfo {
  username: string;
  image_uri: string;
  name: string;
  url: string;
  follower_count: number;
  following_count: number;
  repo_count: number;
  star_count: number;
  bio?: string;
  fetch_pending: boolean;
}

interface HackathonProjectInfo {
  name: string;
  description: string;
  monad_integration: string;
  github_url: string;
  demo_video_url: string;
  agent_moltbook_url?: string;
  github_star_count: number;
  github_fork_count: number;
  github_description?: string;
  github_topics?: string[];
  github_language?: string;
}
```

---

## 사용 예시

### cURL

```bash
# 기본 검색
curl "https://api.example.com/search/PUMP"

# 페이지네이션과 함께 검색
curl "https://api.example.com/search/PUMP?page=1&limit=20&direction=DESC"

# 주소로 검색
curl "https://api.example.com/search/0x1234567890abcdef1234567890abcdef12345678"
```

### JavaScript/TypeScript

```typescript
async function search(name: string, options?: SearchQuery): Promise<SearchResponse> {
  const params = new URLSearchParams();
  if (options?.page) params.set('page', options.page.toString());
  if (options?.limit) params.set('limit', options.limit.toString());
  if (options?.direction) params.set('direction', options.direction);

  const response = await fetch(
    `https://api.example.com/search/${encodeURIComponent(name)}?${params}`
  );

  if (!response.ok) {
    throw new Error(`Search failed: ${response.status}`);
  }

  return response.json();
}

// 사용 예시
const result = await search('PUMP', { page: 1, limit: 20 });
console.log('Found tokens:', result.token_result.total_count);
console.log('Found accounts:', result.account_result.total_count);
```
