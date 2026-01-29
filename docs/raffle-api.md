# Raffle API 문서

## 개요

Raffle API는 래플(추첨) 이벤트 관련 기능을 제공하는 API입니다.

- **래플 자격 확인**: 사용자의 래플 참여 자격 및 현재 엔트리 수 조회
- **래플 결과 확인**: 특정 라운드의 래플 엔트리 및 당첨 정보 조회
- **현재 라운드 조회**: 진행 중인 래플 라운드 정보 조회

---

## API 엔드포인트

### 1. 래플 자격 확인 (`GET /raffle/eligible`)

인증된 사용자의 래플 참여 자격 및 현재 라운드 엔트리 수를 반환합니다.

#### 요청
- **Method**: `GET`
- **인증**: 필수 (Session Cookie)

#### 응답
```json
{
  "is_eligible": true,
  "count": 5
}
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `is_eligible` | boolean | 래플 참여 자격 여부 |
| `count` | number | 현재 활성 라운드의 엔트리 수 |

#### 에러 응답
- `401`: 인증 실패 (세션 쿠키 없음 또는 만료)
- `500`: 내부 서버 에러

---

### 2. 래플 결과 확인 (`GET /raffle/check`)

특정 라운드의 래플 엔트리 및 당첨 정보를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 필수 (Session Cookie)

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `round` | number | O | 조회할 라운드 번호 |

#### 요청 예시
```
GET /raffle/check?round=1
```

#### 응답
```json
{
  "is_validate": true,
  "round": {
    "round": 1,
    "start_at": 1704067200,
    "end_at": 1704153600
  },
  "account_id": "0x1234567890abcdef...",
  "prizes": {
    "total_monad": "100000000000000000000",
    "total_hype": "50000000000000000000"
  }
}
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `is_validate` | boolean | 해당 라운드에 래플 엔트리가 있는지 여부 |
| `round` | object | 라운드 정보 |
| `round.round` | number | 라운드 번호 |
| `round.start_at` | number | 라운드 시작 타임스탬프 (Unix epoch) |
| `round.end_at` | number | 라운드 종료 타임스탬프 (Unix epoch) |
| `account_id` | string | 사용자 지갑 주소 |
| `prizes` | object | 당첨 금액 정보 |
| `prizes.total_monad` | string | Monad 래플 당첨 총액 (wei 단위) |
| `prizes.total_hype` | string | Hype 래플 당첨 총액 (wei 단위) |

#### 에러 응답
- `400`: 잘못된 요청 (round 파라미터 누락 또는 유효하지 않음)
- `401`: 인증 실패
- `500`: 내부 서버 에러

---

### 3. 현재 라운드 조회 (`GET /raffle/round`)

현재 진행 중인 래플 라운드 정보를 반환합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### 응답
```json
{
  "round": 1,
  "status": "ACTIVE",
  "start_at": 1704067200,
  "end_at": 1704153600
}
```

활성 라운드가 없는 경우:
```json
null
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `round` | number | 라운드 번호 |
| `status` | string | 라운드 상태 (`ACTIVE` 또는 `COMPLETED`) |
| `start_at` | number | 라운드 시작 타임스탬프 (Unix epoch) |
| `end_at` | number | 라운드 종료 타임스탬프 (Unix epoch) |

#### 에러 응답
- `500`: 내부 서버 에러

---

## TypeScript Interfaces

### 요청 타입

```typescript
// GET /raffle/check
interface RaffleCheckQuery {
  round: number;  // 조회할 라운드 번호
}
```

### 응답 타입

```typescript
// GET /raffle/eligible
interface RaffleStatusResponse {
  is_eligible: boolean;  // 래플 참여 자격 여부
  count: number;         // 현재 활성 라운드의 엔트리 수
}

// GET /raffle/check
interface RaffleCheckResponse {
  is_validate: boolean;  // 해당 라운드에 엔트리 존재 여부
  round: RaffleRound;    // 라운드 정보
  account_id: string;    // 사용자 지갑 주소
  prizes: RafflePrizes;  // 당첨 금액 정보
}

interface RaffleRound {
  round: number;    // 라운드 번호
  start_at: number; // 라운드 시작 타임스탬프
  end_at: number;   // 라운드 종료 타임스탬프
}

interface RafflePrizes {
  total_monad: string;  // Monad 래플 당첨 총액 (wei 단위)
  total_hype: string;   // Hype 래플 당첨 총액 (wei 단위)
}

// GET /raffle/round
interface RaffleRoundResponse {
  round: number;    // 라운드 번호
  status: string;   // 라운드 상태 ("ACTIVE" | "COMPLETED")
  start_at: number; // 라운드 시작 타임스탬프
  end_at: number;   // 라운드 종료 타임스탬프
}
```

---

## 인증

래플 API 중 일부 엔드포인트는 세션 쿠키 기반 인증이 필요합니다.

| 엔드포인트 | 인증 필요 |
|------------|-----------|
| `GET /raffle/eligible` | O |
| `GET /raffle/check` | O |
| `GET /raffle/round` | X |

인증이 필요한 API는 요청 시 `session` 쿠키가 포함되어야 합니다.
