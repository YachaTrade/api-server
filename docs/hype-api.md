# Hype API 문서

## 개요

Hype API는 토큰 투표 및 리워드 시스템을 위한 API입니다.

- **Hype Token**: epoch별 투표 대상 토큰 조회
- **Hype Point**: 사용자의 라운드 포인트 및 누적 포인트 조회
- **Hype Epoch**: 현재 epoch 정보 조회
- **Vote**: 토큰에 포인트 투표
- **Community Treasury**: 커뮤니티 트레저리 잔액 조회

---

## API 엔드포인트

### 1. Hype 토큰 조회 (`GET /hype/token`)

특정 epoch의 Hype 토큰 목록을 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `epoch` | integer | X | 조회할 epoch 번호 (미입력 시 현재 epoch) |

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
        "is_cto": false
      },
      "hype_info": {
        "vote": "1000000",
        "holder_count": 150,
        "market_cap": "500000000000000000000",
        "market_cap_usd": "1500.00",
        "reward_amount": "100000000000000000"
      }
    }
  ],
  "total_count": 10
}
```

#### 에러 응답
- `400`: 잘못된 요청
- `500`: 내부 서버 에러

---

### 2. 최신 Hype 토큰 조회 (`GET /hype/token/latest`)

현재 ACTIVE 상태이거나 가장 최근 COMPLETE 상태의 Hype 토큰 목록을 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### 응답
```json
{
  "tokens": [
    {
      "token_info": { ... },
      "hype_info": {
        "vote": "1000000",
        "holder_count": 150,
        "market_cap": "500000000000000000000",
        "market_cap_usd": "1500.00",
        "reward_amount": "100000000000000000"
      }
    }
  ],
  "total_count": 10
}
```

#### 에러 응답
- `400`: 잘못된 요청
- `500`: 내부 서버 에러

---

### 3. Hype 포인트 조회 (`GET /hype/point`)

현재 사용자의 라운드 포인트 및 누적 Hype 포인트를 조회합니다. **인증 필요**

#### 요청
- **Method**: `GET`
- **인증**: 필수 (Session Cookie)

#### 응답
```json
{
  "account_id": "0x...",
  "round_point": "1000",
  "hype_point": "5000"
}
```

#### 필드 설명
- `account_id`: 사용자 지갑 주소
- `round_point`: 현재 라운드에서 사용 가능한 포인트
- `hype_point`: 총 누적 Hype 포인트

#### 에러 응답
- `400`: 잘못된 요청
- `401`: 인증 실패
- `500`: 내부 서버 에러

---

### 4. Hype Epoch 조회 (`GET /hype/epoch`)

현재 Hype epoch 정보를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### 응답
```json
{
  "epoch": 5,
  "start_at": 1704067200,
  "end_at": 1704153600,
  "status": "ACTIVE"
}
```

#### 필드 설명
- `epoch`: 현재 epoch 번호
- `start_at`: epoch 시작 시간 (Unix timestamp)
- `end_at`: epoch 종료 시간 (Unix timestamp)
- `status`: epoch 상태 (`ACTIVE`, `COMPLETE` 등)

#### 에러 응답
- `400`: 잘못된 요청
- `500`: 내부 서버 에러

---

### 5. 투표 히스토리 조회 (`GET /hype/vote_history`)

사용자의 투표 히스토리를 페이지네이션으로 조회합니다. **인증 필요**

#### 요청
- **Method**: `GET`
- **인증**: 필수 (Session Cookie)

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 (최소 1) |
| `limit` | integer | 10 | 페이지당 항목 수 (1-100) |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |

#### 응답
```json
{
  "history": [
    {
      "epoch": 5,
      "is_live": true,
      "token_info": {
        "token_id": "0x...",
        "name": "Token Name",
        "symbol": "TKN",
        ...
      },
      "vote_amount": "100",
      "reward_amount": "50000000000000000",
      "claimable": true,
      "proof": ["0x...", "0x...", "0x..."]
    }
  ],
  "total_count": 25
}
```

#### 필드 설명
- `epoch`: 투표한 epoch 번호
- `is_live`: 해당 epoch가 현재 진행 중인지 여부
- `token_info`: 투표한 토큰 정보
- `vote_amount`: 투표한 포인트 수량
- `reward_amount`: 받을 리워드 수량
- `claimable`: 리워드 클레임 가능 여부
- `proof`: Merkle proof (클레임 시 필요)

#### 에러 응답
- `400`: 잘못된 요청 (잘못된 페이지네이션 파라미터)
- `401`: 인증 실패
- `500`: 내부 서버 에러

---

### 6. 리워드 추가 히스토리 조회 (`GET /hype/reward_add_history`)

토큰에 추가된 리워드 히스토리를 조회합니다. **인증 필요**

#### 요청
- **Method**: `GET`
- **인증**: 필수 (Session Cookie)

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 (최소 1) |
| `limit` | integer | 10 | 페이지당 항목 수 (1-100) |
| `direction` | string | DESC | 정렬 방향 (ASC/DESC) |

#### 응답
```json
{
  "history": [
    {
      "epoch": 5,
      "token_info": {
        "token_id": "0x...",
        "name": "Token Name",
        "symbol": "TKN",
        ...
      },
      "amount": "100000000000000000",
      "total_amount": "500000000000000000",
      "created_at": 1704067200,
      "transaction_hash": "0x..."
    }
  ],
  "total_count": 10
}
```

#### 필드 설명
- `epoch`: 리워드가 추가된 epoch
- `token_info`: 리워드가 추가된 토큰 정보
- `amount`: 추가된 리워드 수량
- `total_amount`: 총 누적 리워드 수량
- `created_at`: 리워드 추가 시간 (Unix timestamp)
- `transaction_hash`: 트랜잭션 해시

#### 에러 응답
- `400`: 잘못된 요청
- `401`: 인증 실패
- `500`: 내부 서버 에러

---

### 7. 투표하기 (`POST /hype/vote`)

토큰에 포인트를 투표합니다. **인증 필요**

#### 요청
- **Method**: `POST`
- **Content-Type**: `application/json`
- **인증**: 필수 (Session Cookie)

#### Request Body
```json
{
  "token_id": "0x1234567890abcdef...",
  "amount": "100"
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `token_id` | string | O | 투표할 토큰 주소 (EVM 형식) |
| `amount` | string | O | 투표할 포인트 수량 (양의 정수) |

#### 검증 규칙
- `token_id`: 유효한 EVM 주소 형식이어야 함
- `amount`: 양의 정수여야 함 (0보다 커야 함)

#### 응답
```json
{
  "account_id": "0x...",
  "round_point": "900",
  "hype_point": "5100",
  "token_vote": "100"
}
```

#### 필드 설명
- `account_id`: 투표한 사용자 지갑 주소
- `round_point`: 투표 후 남은 라운드 포인트
- `hype_point`: 투표 후 총 Hype 포인트
- `token_vote`: 해당 토큰에 투표한 총 포인트

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 token_id, amount)
- `401`: 인증 실패
- `500`: 내부 서버 에러

---

### 8. 커뮤니티 트레저리 조회 (`GET /hype/community_treasury`)

커뮤니티 트레저리의 현재 잔액을 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### 응답
```json
{
  "amount": "1000000000000000000000"
}
```

#### 에러 응답
- `400`: 잘못된 요청
- `500`: 내부 서버 에러

---

### 9. 총 Hype 포인트 조회 (`GET /hype/total_hype_point`)

시스템 전체의 총 Hype 포인트를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### 응답
```json
{
  "amount": "50000000"
}
```

#### 에러 응답
- `400`: 잘못된 요청
- `500`: 내부 서버 에러

---

## TypeScript Interfaces

### 요청 타입

```typescript
// GET /hype/token
interface HypeTokenQuery {
  epoch?: number;  // 조회할 epoch (선택)
}

// GET /hype/vote_history, /hype/reward_add_history
interface PaginationParams {
  page?: number;       // default: 1, min: 1
  limit?: number;      // default: 10, min: 1, max: 100
  direction?: string;  // "ASC" | "DESC", default: "DESC"
}

// POST /hype/vote
interface HypeVoteRequest {
  token_id: string;  // EVM 주소 형식
  amount: string;    // 양의 정수 문자열
}
```

### 응답 타입

```typescript
// GET /hype/token, /hype/token/latest
interface HypeTokenResponse {
  tokens: HypeToken[];
  total_count: number;
}

interface HypeToken {
  token_info: TokenInfo;
  hype_info: HypeInfo;
}

interface HypeInfo {
  vote: string;           // 총 투표 수
  holder_count: number;   // 홀더 수
  market_cap: string;     // 마켓캡 (wei)
  market_cap_usd: string; // 마켓캡 (USD)
  reward_amount: string;  // 리워드 수량
}

// GET /hype/point
interface HypePointResponse {
  account_id: string;    // 지갑 주소
  round_point: string;   // 현재 라운드 포인트
  hype_point: string;    // 총 Hype 포인트
}

// GET /hype/epoch
interface HypeEpochResponse {
  epoch: number;         // epoch 번호
  start_at: number;      // 시작 시간 (Unix timestamp)
  end_at: number;        // 종료 시간 (Unix timestamp)
  status: string;        // "ACTIVE" | "COMPLETE" 등
}

// GET /hype/vote_history
interface HypeVoteHistoryResponse {
  history: HypeVoteHistory[];
  total_count: number;
}

interface HypeVoteHistory {
  epoch: number;
  is_live: boolean;
  token_info: TokenInfo;
  vote_amount: string;
  reward_amount: string;
  claimable: boolean;
  proof: string[];
}

// GET /hype/reward_add_history
interface HypeRewardAddHistoryResponse {
  history: RewardAdd[];
  total_count: number;
}

interface RewardAdd {
  epoch: number;
  token_info: TokenInfo;
  amount: string;
  total_amount: string;
  created_at: number;
  transaction_hash: string;
}

// POST /hype/vote
interface HypeVoteResponse {
  account_id: string;
  round_point: string;
  hype_point: string;
  token_vote: string;
}

// GET /hype/community_treasury, /hype/total_hype_point
interface AmountResponse {
  amount: string;
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
```

---

## API 엔드포인트 요약

| Method | Path | 인증 | 설명 |
|--------|------|------|------|
| GET | `/hype/token` | X | 특정 epoch의 Hype 토큰 조회 |
| GET | `/hype/token/latest` | X | 최신 Hype 토큰 조회 |
| GET | `/hype/point` | O | 사용자 포인트 조회 |
| GET | `/hype/epoch` | X | 현재 epoch 정보 조회 |
| GET | `/hype/vote_history` | O | 투표 히스토리 조회 |
| GET | `/hype/reward_add_history` | O | 리워드 추가 히스토리 조회 |
| POST | `/hype/vote` | O | 토큰에 투표 |
| GET | `/hype/community_treasury` | X | 커뮤니티 트레저리 잔액 조회 |
| GET | `/hype/total_hype_point` | X | 총 Hype 포인트 조회 |
