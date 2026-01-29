# Hackathon API 문서

## 개요

Hackathon API는 해커톤 프로젝트 등록 및 조회를 위한 API입니다.

- **프로젝트 등록**: GitHub 정보를 자동으로 가져와 DB에 저장
- **토큰 목록 조회**: 해커톤에 등록된 토큰 ID 목록 반환

---

## DB 스키마

### hackathon (화이트리스트)
```sql
CREATE TABLE hackathon (
    token_id TEXT PRIMARY KEY REFERENCES token(token_id),
    created_at BIGINT NOT NULL
);
```

### hackathon_creator (개발자 정보)
```sql
CREATE TABLE hackathon_creator (
    github_id TEXT PRIMARY KEY,
    image_uri TEXT,              -- GitHub 프로필 이미지
    name TEXT,                   -- GitHub 이름
    github_url TEXT,             -- GitHub 프로필 URL
    follower_count INTEGER,      -- 팔로워 수
    following_count INTEGER,     -- 팔로잉 수
    repo_count INTEGER,          -- 공개 레포 수
    star_count INTEGER,          -- 총 스타 수 (모든 레포 합계)
    bio TEXT,                    -- GitHub 바이오
    twitter TEXT NOT NULL,       -- 트위터 (필수)
    discord TEXT,                -- 디스코드
    telegram TEXT,               -- 텔레그램
    linkedin TEXT,               -- 링크드인
    account_id TEXT NOT NULL,    -- 지갑 주소
    created_at BIGINT NOT NULL
);
```

### hackathon_project (프로젝트 정보)
```sql
CREATE TABLE hackathon_project (
    token_id TEXT PRIMARY KEY REFERENCES hackathon(token_id),
    github_id TEXT NOT NULL REFERENCES hackathon_creator(github_id),
    github_url TEXT NOT NULL,    -- 프로젝트 GitHub URL
    name TEXT NOT NULL,          -- 프로젝트 이름
    description TEXT NOT NULL,   -- 프로젝트 설명
    keywords TEXT NOT NULL,      -- 키워드 (쉼표 구분)
    screenshot_uri TEXT NOT NULL,-- 스크린샷 이미지 URL
    website TEXT,                -- 웹사이트 URL
    youtube TEXT,                -- YouTube URL
    star_count INTEGER,          -- 프로젝트 스타 수
    fork_count INTEGER,          -- 프로젝트 포크 수
    topics TEXT,                 -- GitHub 토픽 (쉼표 구분)
    language TEXT,               -- 주요 프로그래밍 언어
    created_at BIGINT NOT NULL
);
```

---

## API 엔드포인트

### 1. 해커톤 프로젝트 등록 (`POST /cms/hackathon/register`)

해커톤 프로젝트를 등록합니다. **Admin 권한 필요**

#### 요청
- **Method**: `POST`
- **Content-Type**: `application/json`
- **인증**: 필수 (Admin only)

#### Request Body
```json
{
  "github_id": "MonkeyGyu",
  "twitter": "@username",
  "discord": "username#1234",
  "telegram": "@username",
  "linkedin": "https://linkedin.com/in/username",
  "project_github_url": "https://github.com/owner/repo",
  "project_name": "My Awesome Project",
  "project_description": "A decentralized application for...",
  "keywords": "defi,nft,trading",
  "screenshot_uri": "https://storage.nadapp.net/screenshots/uuid.png",
  "website": "https://myproject.com",
  "youtube": "https://youtube.com/watch?v=xxxxx",
  "token_id": "0x1234567890abcdef...",
  "account_id": "0xabcdef1234567890..."
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `github_id` | string | O | GitHub 사용자명 |
| `twitter` | string | O | 트위터 핸들 |
| `discord` | string | X | 디스코드 핸들 |
| `telegram` | string | X | 텔레그램 핸들 |
| `linkedin` | string | X | 링크드인 URL |
| `project_github_url` | string | O | 프로젝트 GitHub URL (`https://github.com/`으로 시작) |
| `project_name` | string | O | 프로젝트 이름 |
| `project_description` | string | O | 프로젝트 설명 |
| `keywords` | string | O | 키워드 (쉼표 구분) |
| `screenshot_uri` | string | O | 스크린샷 이미지 URL |
| `website` | string | X | 웹사이트 URL |
| `youtube` | string | X | YouTube URL |
| `token_id` | string | O | 토큰 컨트랙트 주소 (EVM 형식) |
| `account_id` | string | O | 개발자 지갑 주소 (EVM 형식) |

#### 처리 과정

1. Admin 권한 확인
2. `token_id`가 `token` 테이블에 존재하는지 확인
3. GitHub API 호출 → Creator 정보 가져오기
   - 프로필 이미지, 이름, 팔로워/팔로잉 수, 레포 수, 총 스타 수, 바이오
4. GitHub API 호출 → Project 정보 가져오기
   - 스타 수, 포크 수, 토픽, 주요 언어
5. 트랜잭션으로 DB Insert
   - `hackathon` 테이블 (화이트리스트)
   - `hackathon_creator` 테이블 (upsert)
   - `hackathon_project` 테이블 (upsert)

#### 응답
```json
{
  "success": true,
  "token_id": "0x1234567890abcdef..."
}
```

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 token_id, 필수 필드 누락, token 미존재)
- `401`: 인증 실패 (Admin 권한 없음)
- `500`: 내부 서버 에러 (GitHub API 실패, DB 에러)

---

### 2. 해커톤 토큰 목록 조회 (`GET /token/hackathon`)

등록된 모든 해커톤 토큰 ID 목록을 반환합니다.

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

### 3. 해커톤 토큰 정렬 조회 (`GET /order/hackathon`)

해커톤 토큰 목록을 마켓캡 순으로 정렬하여 반환합니다. 기존 Order API와 동일한 응답 형식입니다.

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
          "creator": { ... },
          "project": { ... }
        }
      },
      "market_info": {
        "market_type": "CURVE",
        "token_id": "0x...",
        "market_id": "0x...",
        "token_price": "0.001",
        "native_price": "3000",
        "price": "0.000001",
        "price_usd": "0.003",
        "total_supply": "1000000000",
        "reserve_native": "100",
        "reserve_token": "500000000",
        "volume": "50000",
        "ath_price": "0.000002",
        "holder_count": 150
      },
      "percent": 15.5
    }
  ],
  "total_count": 25
}
```

#### 특징
- 마켓캡(price) 기준 정렬
- `hackathon_info` 필드에 개발자/프로젝트 정보 자동 포함
- 24시간 가격 변동률(`percent`) 포함

#### 에러 응답
- `400`: 잘못된 페이지네이션 파라미터
- `500`: 내부 서버 에러

---

### 4. 해커톤 토큰 상세 조회 (기존 API 활용)

`GET /token/:token_id` API를 사용하면 `hackathon_info` 필드에 해커톤 정보가 포함됩니다.

#### 응답 예시 (hackathon_info 부분)
```json
{
  "token_info": {
    "token_id": "0x...",
    "name": "Token Name",
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
  }
}
```

---

## TypeScript Interfaces

### 요청 타입

```typescript
// POST /cms/hackathon/register
interface RegisterHackathonRequest {
  github_id: string;
  twitter: string;
  discord?: string;
  telegram?: string;
  linkedin?: string;
  project_github_url: string;
  project_name: string;
  project_description: string;
  keywords: string;
  screenshot_uri: string;
  website?: string;
  youtube?: string;
  token_id: string;
  account_id: string;
}

// GET /order/hackathon
interface OrderQuery {
  page?: number;       // default: 1
  limit?: number;      // default: 20
  direction?: string;  // "ASC" | "DESC", default: "DESC"
  is_nsfw?: boolean;   // default: false
}
```

### 응답 타입

```typescript
// POST /cms/hackathon/register
interface RegisterHackathonResponse {
  success: boolean;
  token_id: string;
}

// GET /token/hackathon
interface HackathonTokenListResponse {
  token_ids: string[];
}

// GET /order/hackathon
interface OrderTokenResponse {
  tokens: OrderToken[];
  total_count: number;
}

interface OrderToken {
  token_info: TokenInfo;
  market_info: MarketInfo;
  percent: number;  // 24h 가격 변동률
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
  token_price: string;
  native_price: string;
  price: string;
  price_usd: string;
  price_native: string;
  total_supply: string;
  reserve_native: string;
  reserve_token: string;
  volume: string;
  ath_price: string;
  ath_price_usd: string;
  ath_price_native: string;
  holder_count: number;
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

---

## GitHub API 연동

### 사용되는 GitHub API

1. **User API**: `GET https://api.github.com/users/{github_id}`
   - 프로필 정보 (avatar, name, followers, following, public_repos, bio)

2. **User Repos API**: `GET https://api.github.com/users/{github_id}/repos`
   - 총 스타 수 계산 (모든 레포의 stargazers_count 합계)

3. **Repository API**: `GET https://api.github.com/repos/{owner}/{repo}`
   - 프로젝트 정보 (stars, forks, topics, language)

### Rate Limit

- **인증 없음**: 60 requests/hour
- **Personal Access Token**: 5,000 requests/hour

현재 환경변수 `GITHUB_TOKEN`에 설정된 토큰으로 인증하여 5,000 requests/hour 사용 중.

---

## 환경변수

```env
# GitHub API Token (필수)
GITHUB_TOKEN=ghp_xxxxxxxxxxxxx
```
