# Leaderboard API 문서

## 개요

Leaderboard API는 사용자 순위 조회를 위한 API입니다.

- **PnL 리더보드**: 사용자의 손익(Profit and Loss) 순위 조회

---

## API 엔드포인트

### 1. PnL 리더보드 조회 (`GET /leaderboard/pnl`)

사용자의 손익(Profit and Loss) 순위를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Query Parameters

| 파라미터 | 타입 | 기본값 | 설명 |
|----------|------|--------|------|
| `page` | integer | 1 | 페이지 번호 (최소 1) |
| `limit` | integer | 10 | 페이지당 항목 수 (최소 1, 최대 100) |

#### 응답
```json
{
  "ranks": [
    {
      "rank": 1,
      "account_info": {
        "account_id": "0x1234567890abcdef...",
        "nickname": "ProfitMaster",
        "bio": "Professional trader",
        "image_uri": "https://storage.nadapp.net/profiles/uuid.png"
      },
      "pnl": {
        "realized_native": "150.5",
        "unrealized_native": "50.25",
        "total_native": "200.75",
        "native_percent": "125.5",
        "realized_usd": "450000",
        "unrealized_usd": "150750",
        "total_usd": "600750",
        "usd_percent": "125.5"
      }
    },
    {
      "rank": 2,
      "account_info": {
        "account_id": "0xabcdef1234567890...",
        "nickname": "Trader2",
        "bio": "Swing trader",
        "image_uri": "https://storage.nadapp.net/profiles/uuid2.png"
      },
      "pnl": {
        "realized_native": "100.0",
        "unrealized_native": "30.0",
        "total_native": "130.0",
        "native_percent": "85.0",
        "realized_usd": "300000",
        "unrealized_usd": "90000",
        "total_usd": "390000",
        "usd_percent": "85.0"
      }
    }
  ],
  "total_count": 500,
  "last_updated_at": 1706534400
}
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `ranks` | array | 순위 목록 |
| `ranks[].rank` | integer | 순위 |
| `ranks[].account_info` | object | 계정 정보 |
| `ranks[].pnl` | object | PnL 정보 |
| `total_count` | integer | 전체 참여자 수 |
| `last_updated_at` | integer | 마지막 업데이트 시간 (Unix timestamp) |

#### PnL 필드 상세

| 필드 | 타입 | 설명 |
|------|------|------|
| `realized_native` | string | 실현 손익 (MON) |
| `unrealized_native` | string | 미실현 손익 (MON) |
| `total_native` | string | 총 손익 (MON) = realized + unrealized |
| `native_percent` | string | 수익률 % (native 기준, total 기준) |
| `realized_usd` | string | 실현 손익 (USD) |
| `unrealized_usd` | string | 미실현 손익 (USD) |
| `total_usd` | string | 총 손익 (USD) = realized + unrealized |
| `usd_percent` | string | 수익률 % (USD 기준, total 기준) |

#### 에러 응답
- `500`: 내부 서버 에러

---

## TypeScript Interfaces

### 요청 타입

```typescript
// GET /leaderboard/pnl
interface LeaderboardQuery {
  page?: number;   // default: 1, minimum: 1
  limit?: number;  // default: 10, minimum: 1, maximum: 100
}
```

### 응답 타입

```typescript
// GET /leaderboard/pnl
interface PnlLeaderboardResponse {
  ranks: PnlLeaderboardEntry[];
  total_count: number;
  last_updated_at: number;
}

interface PnlLeaderboardEntry {
  rank: number;
  account_info: AccountInfo;
  pnl: Pnl;
}

interface Pnl {
  /** 실현 손익 (MON) */
  realized_native: string;
  /** 미실현 손익 (MON) */
  unrealized_native: string;
  /** 총 손익 (MON) = realized + unrealized */
  total_native: string;
  /** 수익률 % (native 기준, total 기준) */
  native_percent: string;
  /** 실현 손익 (USD) */
  realized_usd: string;
  /** 미실현 손익 (USD) */
  unrealized_usd: string;
  /** 총 손익 (USD) = realized + unrealized */
  total_usd: string;
  /** 수익률 % (USD 기준, total 기준) */
  usd_percent: string;
}

// 공통 타입
interface AccountInfo {
  account_id: string;
  nickname: string;
  bio: string;
  image_uri: string;
}
```

---

## 참고사항

### 페이지네이션

- 모든 리더보드 API는 페이지네이션을 지원합니다.
- `page`는 1부터 시작합니다.
- `limit`의 기본값은 10이며, 최대 100까지 설정 가능합니다.

### 데이터 갱신

- 리더보드 데이터는 주기적으로 갱신됩니다.
- `last_updated_at` 필드를 통해 마지막 갱신 시간을 확인할 수 있습니다.

### 숫자 표현

- 금액 관련 값은 정밀도를 위해 문자열로 반환됩니다.
- 클라이언트에서 적절한 파싱 및 포맷팅이 필요합니다.
