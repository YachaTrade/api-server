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
  "total_usd_volume": "12345.67"
}
```

- `total_usd_volume`: swap 테이블에서 실시간 집계된 USD 거래량 (NUMERIC → String)

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
    "1": "100",
    "2": "500",
    "3": "1000",
    "4": "5000",
    "5": "10000"
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
      "amount": "1000",
      "usd_value": "25000.50"
    },
    {
      "token_id": "0xabcdef1234567890abcdef1234567890abcdef12",
      "amount": "5000",
      "usd_value": "1200.00"
    }
  ]
}
```

- `token_id`: 토큰 컨트랙트 주소
- `amount`: 원시 토큰 수량
- `usd_value`: USD 환산 가치 (Rust에서 계산)

**USD 가치 계산 로직:**
- WMON: `amount * price.price` (MON/USD)
- 기타 토큰: `amount * market.price * price.price` (token→MON→USD)

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
