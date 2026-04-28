# New Event API 문서

## 개요

New Event API는 실시간으로 발생하는 최신 이벤트(매수/매도/토큰 생성)를 조회하기 위한 API입니다.

- **최신 이벤트 조회**: 최근 발생한 Buy, Sell, Create 이벤트를 반환
- **캐싱 지원**: Redis를 통한 캐싱으로 빠른 응답 제공

---

## API 엔드포인트

### 1. 최신 이벤트 조회 (`GET /new_event`)

최근 발생한 이벤트(Buy/Sell/Create) 목록을 반환합니다.

#### 요청
- **Method**: `GET`
- **Path**: `/new_event`
- **인증**: 불필요

#### 응답
```json
{
  "new_events": [
    {
      "type": "BUY",
      "amount": "1000000000000000000",
      "token_info": {
        "token_id": "0x1234567890abcdef...",
        "name": "Token Name",
        "symbol": "TKN",
        "image_uri": "https://storage.nadapp.net/images/...",
        "description": "Token description",
        "is_graduated": false,
        "is_nsfw": false,
        "twitter": "https://x.com/...",
        "telegram": "https://t.me/...",
        "website": "https://example.com",
        "created_at": 1234567890,
        "creator": {
          "account_id": "0xabcdef1234567890...",
          "nickname": "Creator Name",
          "bio": "Creator bio",
          "image_uri": "https://storage.nadapp.net/profiles/..."
        },
        "is_cto": false,
        "version": "V1"
      },
      "account_info": {
        "account_id": "0xabcdef1234567890...",
        "nickname": "Buyer Name",
        "bio": "Buyer bio",
        "image_uri": "https://storage.nadapp.net/profiles/..."
      }
    },
    {
      "type": "SELL",
      "amount": "500000000000000000",
      "token_info": { ... },
      "account_info": { ... }
    },
    {
      "type": "CREATE",
      "amount": "0",
      "token_info": { ... },
      "account_info": { ... }
    }
  ]
}
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `new_events` | NewEvent[] | 최신 이벤트 목록 |

**NewEvent 객체:**

| 필드 | 타입 | 설명 |
|------|------|------|
| `type` | string | 이벤트 타입 (`BUY`, `SELL`, `CREATE`) |
| `amount` | string | 거래 금액 (wei 단위) |
| `token_info` | TokenInfo | 토큰 정보 |
| `account_info` | AccountInfo | 이벤트 발생 계정 정보 |

#### 에러 응답
- `500`: 내부 서버 에러

---

## TypeScript Interfaces

### 요청 타입

```typescript
// GET /new_event - Query Parameters 없음
// 별도의 요청 파라미터가 필요하지 않습니다.
```

### 응답 타입

```typescript
// GET /new_event
interface NewEventResponse {
  new_events: NewEvent[];
}

type EventType = "BUY" | "SELL" | "CREATE";

interface NewEvent {
  type: EventType;
  amount: string;
  token_info: TokenInfo;
  account_info: AccountInfo;
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
  version: TokenVersion;
}

type TokenVersion = "V1" | "V2";

interface AccountInfo {
  account_id: string;
  nickname: string;
  bio: string;
  image_uri: string;
}
```

---

## 이벤트 타입 설명

| 타입 | 설명 |
|------|------|
| `BUY` | 토큰 매수 이벤트 |
| `SELL` | 토큰 매도 이벤트 |
| `CREATE` | 새 토큰 생성 이벤트 |

---

## 캐싱

- Redis를 통해 응답이 캐싱됩니다.
- 캐시가 존재할 경우 DB 조회 없이 캐시된 데이터를 반환합니다.
- 캐시 미스 시 DB에서 조회 후 캐시에 저장합니다.
