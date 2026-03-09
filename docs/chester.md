# Chester API

Chester는 트레이딩 볼륨 기반 체스트 보상 이벤트 시스템입니다.
사용자가 활성 라운드 기간 동안 누적한 거래량에 따라 체스트 레벨을 달성하고 보상을 받을 수 있습니다.

## Endpoints

### GET /chester/volume/:account_id

계정의 활성 라운드 내 누적 USD 거래량을 실시간으로 조회합니다.

- **인증**: 불필요
- **Path Parameter**: `account_id` (String) - 계정 주소

**Response 200**
```json
{
  "volume_usd": "12345.67",
  "fee_usd": "123.45"
}
```

- `volume_usd`: swap 테이블에서 실시간 집계된 USD 거래량
- `fee_usd`: point_history 테이블에서 실시간 집계된 USD 수수료

---

### GET /chester/round

현재 ACTIVE 상태인 라운드 정보를 조회합니다.

- **인증**: 불필요

**Response 200**
```json
{
  "round": 1,
  "start_at": 1706745600,
  "end_at": 1707350400,
  "status": "ACTIVE",
  "chest_level_threshold": {
    "1": "1000",
    "2": "6000",
    "3": "16000",
    "4": "116000"
  }
}
```

- `round`: 라운드 번호
- `start_at` / `end_at`: epoch seconds
- `status`: `ACTIVE` | `COMPLETED` | `READY`
- `chest_level_threshold`: 레벨별 USD 볼륨 임계값 (하드코딩)

활성 라운드가 없으면 `null` 반환.

---

### GET /chester/rewards

활성 라운드의 보상 토큰 목록과 USD 가치를 조회합니다.

- **인증**: 불필요

**Response 200**
```json
{
  "rewards": [
    {
      "token_id": "0x760AfE86e5de5fa0Ee542fc7B7B713e1c5425701",
      "name": "MON",
      "symbol": "MON",
      "image_uri": "https://example.com/mon.jpg",
      "amount": "1000",
      "price": "25.00",
      "usd_value": "25000.50"
    }
  ]
}
```

- `token_id`: 토큰 컨트랙트 주소
- `name` / `symbol` / `image_uri`: 토큰 메타정보 (chester_reward_token JOIN)
- `amount`: 원시 토큰 수량
- `price`: 토큰 USD 가격
- `usd_value`: USD 환산 가치 (Rust에서 계산)

**USD 가치 계산 로직:**
- MON: `amount * price.price` (MON/USD, DB 조회)
- APR: `amount * apr_usd` (CoinGecko API)
- 기타: `0`

### GET /chester/box/rewards

유저의 상자별 보상 결과를 조회합니다. 외부 정산 프로세스에서 생성된 머클 프루프 기반 보상 데이터를 반환합니다.

- **인증**: 필수 (세션 쿠키)
- **Query Parameter**: `round` (Optional, i64) - 라운드 번호. 미지정 시 최신 라운드

**Response 200**
```json
{
  "rewards": [
    {
      "round": 1,
      "level": 1,
      "token_id": "0x3bd359C1119dA7Da1D913D1C4D2B7c461115433A",
      "name": "MON",
      "symbol": "MON",
      "image_uri": "https://example.com/mon.jpg",
      "amount": "1000000000000000000",
      "status": "AWAITING",
      "proof": ["0xabc...", "0xdef..."],
      "transaction_hash": null,
      "claimed_at": null
    }
  ]
}
```

- `round`: 라운드 번호
- `level`: 상자 레벨 (1~4)
- `token_id`: 보상 토큰 컨트랙트 주소
- `name` / `symbol` / `image_uri`: 토큰 메타정보 (chester_reward_token JOIN)
- `amount`: 토큰 수량
- `status`: `AWAITING` (미클레임) | `CLAIMED` (클레임 완료)
- `proof`: 머클 프루프 배열 (**KST 2026-03-09 23:59:59 이후 빈 배열 반환**)
- `transaction_hash`: 클레임 트랜잭션 해시 (클레임 전 null)
- `claimed_at`: 클레임 시간 epoch seconds (클레임 전 null)

**캐싱**: `chester:box_rewards:{account_id}:{round}` (TTL 10s)

---

### GET /chester/swap-history/:account_id

활성 라운드 기간 내 계정의 스왑 거래 내역을 조회합니다.

- **인증**: 불필요
- **Path Parameter**: `account_id` (String) - 계정 주소
- **Query Parameters**:
  - `page` (Optional, i64) - 페이지 번호 (기본값: 1)
  - `limit` (Optional, i64) - 페이지당 항목 수 (기본값: 10, 최대: 100)

**Response 200**: `SwapHistoryResponse` (토큰 정보 + 스왑 정보 목록)

---

### GET /chester/reward-history

인증된 계정의 보상 클레임 내역을 조회합니다. `chest_point_distribution` 테이블 기반으로 하이프 포인트와 토큰 보상을 함께 반환합니다.

- **인증**: 필수 (세션 쿠키)
- **Query Parameters**:
  - `page` (Optional, i64) - 페이지 번호 (기본값: 1)
  - `limit` (Optional, i64) - 페이지당 항목 수 (기본값: 10, 최대: 100)

**Response 200**
```json
{
  "histories": [
    {
      "round": 1,
      "level": 1,
      "token_id": "0x3bd359C1119dA7Da1D913D1C4D2B7c461115433A",
      "amount": "1000000000000000000",
      "transaction_hash": "0xabc...",
      "created_at": 1706745600
    },
    {
      "round": 1,
      "level": 1,
      "token_id": "hype",
      "amount": "80",
      "transaction_hash": null,
      "created_at": 1706745600
    }
  ],
  "total_count": 5
}
```

- `round`: 라운드 번호
- `level`: 상자 레벨 (1~4)
- `token_id`: 토큰 컨트랙트 주소 또는 `"hype"` (CHEST 포인트)
- `amount`: 토큰 raw amount 또는 포인트 수량
- `transaction_hash`: 클레임 트랜잭션 해시 (hype 포인트는 null)
- `created_at`: 보상 시간 (epoch seconds)
- `total_count`: 전체 보상 이벤트 수

---

## DB Schema

### chester_round
| Column | Type | Description |
|--------|------|-------------|
| round | BIGINT (PK) | 라운드 번호 |
| start_at | BIGINT | 시작 시간 (epoch) |
| end_at | BIGINT | 종료 시간 (epoch) |
| status | VARCHAR | ACTIVE / COMPLETED / READY |
| created_at | BIGINT | 생성 시간 (epoch) |

- `UNIQUE INDEX`: ACTIVE 상태는 최대 1개
- `chest_level_threshold`: 코드에서 하드코딩 (DB 미저장)

### chester_reward
| Column | Type | Description |
|--------|------|-------------|
| round | BIGINT (FK) | 라운드 번호 |
| token_id | VARCHAR(42) | 토큰 컨트랙트 주소 |
| amount | NUMERIC | 보상 수량 |

- PK: `(round, token_id)`

### chester_box_reward
| Column | Type | Description |
|--------|------|-------------|
| round | BIGINT (FK) | 라운드 번호 |
| level | INT | 상자 레벨 (1~4) |
| token_id | VARCHAR(42) | 보상 토큰 주소 |
| account_id | VARCHAR(42) | 유저 주소 |
| amount | NUMERIC | 토큰 수량 |
| status | VARCHAR | AWAITING / CLAIMED |
| proof | TEXT[] | 머클 프루프 |
| transaction_hash | VARCHAR | 클레임 트랜잭션 해시 |
| claimed_at | BIGINT | 클레임 시간 (epoch) |
| created_at | BIGINT | 생성 시간 (epoch) |

- PK: `(round, level, token_id, account_id)`
- INDEX: `account_id`, `(round, account_id)`
