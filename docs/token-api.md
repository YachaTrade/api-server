# Token API 문서

## 개요

Token API는 토큰 정보 조회 및 토큰 주소 생성을 위한 API입니다.

- **토큰 정보 조회**: 토큰의 기본 정보 및 크리에이터 정보 반환
- **토큰 메타데이터 조회**: 토큰 정보와 마켓 정보를 함께 반환
- **Salt 마이닝**: 특정 접미사를 가진 토큰 주소 생성을 위한 salt 값 계산
- **해커톤 토큰 목록**: 해커톤에 등록된 토큰 ID 목록 반환

---

## API 엔드포인트

### 1. 토큰 정보 조회 (`GET /token/:token`)

특정 토큰의 기본 정보를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `token` | string | O | 토큰 컨트랙트 주소 (EVM 형식) |

#### 응답
```json
{
  "token_info": {
    "token_id": "0x1234567890abcdef...",
    "name": "Token Name",
    "symbol": "TKN",
    "image_uri": "https://storage.nadapp.net/images/uuid.png",
    "description": "Token description",
    "is_graduated": false,
    "is_nsfw": false,
    "twitter": "https://x.com/tokenname",
    "telegram": "https://t.me/tokenname",
    "website": "https://tokenname.com",
    "created_at": 1234567890,
    "creator": {
      "account_id": "0xabcdef1234567890...",
      "nickname": "Creator Name",
      "bio": "Creator bio",
      "image_uri": "https://storage.nadapp.net/profiles/uuid.png"
    },
    "is_cto": false,
    "hackathon_info": null
  }
}
```

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 token ID)
- `500`: 내부 서버 에러

---

### 2. 토큰 메타데이터 조회 (`GET /token/metadata/:token_id`)

토큰의 상세 정보와 마켓 정보를 함께 조회합니다.

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
  "token_info": {
    "token_id": "0x1234567890abcdef...",
    "name": "Token Name",
    "symbol": "TKN",
    "image_uri": "https://storage.nadapp.net/images/uuid.png",
    "description": "Token description",
    "is_graduated": false,
    "is_nsfw": false,
    "twitter": "https://x.com/tokenname",
    "telegram": "https://t.me/tokenname",
    "website": "https://tokenname.com",
    "created_at": 1234567890,
    "creator": {
      "account_id": "0xabcdef1234567890...",
      "nickname": "Creator Name",
      "bio": "Creator bio",
      "image_uri": "https://storage.nadapp.net/profiles/uuid.png"
    },
    "is_cto": false,
    "hackathon_info": null
  },
  "market_info": {
    "market_type": "CURVE",
    "token_id": "0x1234567890abcdef...",
    "market_id": "0xmarketaddress...",
    "reserve_native": "100000000000000000000",
    "reserve_token": "500000000000000000000000000",
    "token_price": "0.001",
    "native_price": "3000",
    "price": "0.000001",
    "price_usd": "0.003",
    "price_native": "0.000001",
    "total_supply": "1000000000000000000000000000",
    "volume": "50000000000000000000000",
    "ath_price": "0.000002",
    "ath_price_usd": "0.006",
    "ath_price_native": "0.000002",
    "holder_count": 150
  }
}
```

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 token ID)
- `500`: 내부 서버 에러

---

### 3. Salt 마이닝 (`POST /token/salt`)

특정 접미사를 가진 토큰 주소를 생성하기 위한 salt 값을 계산합니다.

#### 요청
- **Method**: `POST`
- **Content-Type**: `application/json`
- **인증**: 불필요

#### Request Body
```json
{
  "creator": "0x742d35Cc6634C0532925a3b844Bc9e7595f70143",
  "name": "My Token",
  "symbol": "MTK",
  "metadata_uri": "https://storage.nadapp.net/metadata-94a412d2-b599-4bb0-b026-b14c4036c58c.json"
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `creator` | string | O | 토큰 생성자 지갑 주소 (EVM 형식) |
| `name` | string | O | 토큰 이름 (길이 제한 있음) |
| `symbol` | string | O | 토큰 심볼 (영숫자만 허용, 길이 제한 있음) |
| `metadata_uri` | string | O | 메타데이터 URI (허용된 도메인으로 시작해야 함) |

#### 유효성 검사
- `creator`: 유효한 EVM 주소 형식이어야 함
- `name`: 줄바꿈 문자 포함 불가
- `symbol`: 영숫자만 허용
- `metadata_uri`: 허용된 도메인(`https://storage.nadapp.net/`)으로 시작해야 함

#### 응답
```json
{
  "salt": "0x000000000000000000000000000000000000000000000000000000000000a3f5",
  "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f7777"
}
```

#### 에러 응답
- `400`: 잘못된 요청 (유효성 검사 실패)
- `408`: 요청 타임아웃 (최대 반복 횟수 도달)
- `500`: 내부 서버 에러

---

### 4. 해커톤 토큰 목록 조회 (`GET /token/hackathon`)

해커톤에 등록된 모든 토큰 ID 목록을 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### 응답
```json
{
  "token_ids": [
    "0x1234567890abcdef...",
    "0x5678901234abcdef...",
    "0xabcdef1234567890..."
  ]
}
```

- `created_at` 내림차순 정렬 (최신순)

#### 에러 응답
- `500`: 내부 서버 에러

---

## TypeScript Interfaces

### 요청 타입

```typescript
// POST /token/salt
interface MineSaltRequest {
  /** 토큰 생성자 지갑 주소 */
  creator: string;
  /** 토큰 이름 */
  name: string;
  /** 토큰 심볼 */
  symbol: string;
  /** 메타데이터 URI */
  metadata_uri: string;
}
```

### 응답 타입

```typescript
// GET /token/:token
interface TokenResponse {
  token_info: TokenInfo;
}

// GET /token/metadata/:token_id
interface TokenMetadataResponse {
  token_info: TokenInfo;
  market_info: MarketInfo;
}

// POST /token/salt
interface MineSaltResponse {
  /** 0x prefix가 붙은 salt 값 (hex string) */
  salt: string;
  /** 생성된 토큰 주소 */
  address: string;
}

// POST /token/salt (에러)
interface MineSaltError {
  /** 에러 메시지 */
  error: string;
  /** 시도한 반복 횟수 */
  iterations_attempted?: number;
}

// GET /token/hackathon
interface HackathonTokenListResponse {
  token_ids: string[];
}
```

### 공통 타입

```typescript
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
  /** Native 토큰 reserve */
  reserve_native: string;
  /** Token reserve */
  reserve_token: string;
  /** Token/USD 가격 */
  token_price: string;
  /** MON/USD 가격 */
  native_price: string;
  /** MON/Token 가격 */
  price: string;
  /** USD/Token 가격 */
  price_usd: string;
  /** MON/Token 가격 */
  price_native: string;
  /** 총 공급량 */
  total_supply: string;
  /** 총 거래량 */
  volume: string;
  /** ATH 가격 */
  ath_price: string;
  /** ATH 가격 (USD) */
  ath_price_usd: string;
  /** ATH 가격 (Native) */
  ath_price_native: string;
  /** 홀더 수 */
  holder_count: number;
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

## 엔드포인트 요약

| Method | Path | 설명 |
|--------|------|------|
| GET | `/token/:token` | 토큰 정보 조회 |
| GET | `/token/metadata/:token_id` | 토큰 메타데이터 조회 |
| POST | `/token/salt` | Salt 마이닝 |
| GET | `/token/hackathon` | 해커톤 토큰 목록 조회 |
