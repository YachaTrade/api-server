# Terminal API 문서

## 개요

Terminal API는 GeckoTerminal과 같은 외부 가격 집계 서비스와의 연동을 위한 API입니다.

- **최신 블록 조회**: 현재 인덱싱된 최신 블록 정보 반환
- **Asset 조회**: 토큰(Asset) 정보 조회
- **Pair 조회**: 거래쌍 정보 조회
- **이벤트 조회**: 특정 블록 범위 내의 거래 이벤트 조회
- **메타데이터 조회**: 토큰의 메타데이터 조회

---

## API 엔드포인트

### 1. 최신 블록 조회 (`GET /terminal/latest-block`)

인덱싱된 최신 블록 정보를 반환합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### 응답
```json
{
  "block": {
    "blockNumber": 12345678,
    "blockTimestamp": 1706500000
  }
}
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `block.blockNumber` | number | 블록 번호 |
| `block.blockTimestamp` | number | 블록 타임스탬프 (Unix timestamp) |

#### 에러 응답
- `500`: 내부 서버 에러

---

### 2. Asset 조회 (`GET /terminal/asset`)

토큰(Asset) 정보를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `id` | string | O | Asset ID (토큰 컨트랙트 주소) |

#### 응답
```json
{
  "asset": {
    "id": "0x1234567890abcdef...",
    "name": "Token Name",
    "symbol": "TKN",
    "decimals": 18,
    "totalSupply": "1000000000000000000000000",
    "circulatingSupply": "800000000000000000000000",
    "coinGeckoId": "token-name"
  }
}
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `asset.id` | string | 토큰 컨트랙트 주소 |
| `asset.name` | string | 토큰 이름 |
| `asset.symbol` | string | 토큰 심볼 |
| `asset.decimals` | number | 소수점 자릿수 |
| `asset.totalSupply` | string? | 총 공급량 (wei 단위) |
| `asset.circulatingSupply` | string? | 유통 공급량 (wei 단위) |
| `asset.coinGeckoId` | string? | CoinGecko ID |

#### 에러 응답
- `400`: 잘못된 요청 (id 파라미터 누락)
- `500`: 내부 서버 에러

---

### 3. Pair 조회 (`GET /terminal/pair`)

거래쌍(Pair) 정보를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `id` | string | O | Pair ID (거래쌍 컨트랙트 주소) |

#### 응답
```json
{
  "pair": {
    "id": "0xabcdef1234567890...",
    "dexKey": "nadpump",
    "asset0Id": "0x1111111111111111...",
    "asset1Id": "0x2222222222222222...",
    "createdAtBlockNumber": 12345000,
    "createdAtBlockTimestamp": 1706400000,
    "createdAtTxnId": "0x9876543210abcdef...",
    "creator": "0xcccccccccccccccc...",
    "feeBps": 30
  }
}
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `pair.id` | string | 거래쌍 컨트랙트 주소 |
| `pair.dexKey` | string | DEX 식별자 |
| `pair.asset0Id` | string | 첫 번째 토큰 주소 |
| `pair.asset1Id` | string | 두 번째 토큰 주소 |
| `pair.createdAtBlockNumber` | number? | 생성 블록 번호 |
| `pair.createdAtBlockTimestamp` | number? | 생성 시간 (Unix timestamp) |
| `pair.createdAtTxnId` | string? | 생성 트랜잭션 해시 |
| `pair.creator` | string? | 생성자 주소 |
| `pair.feeBps` | number? | 수수료 (basis points, 1bp = 0.01%) |

#### 에러 응답
- `400`: 잘못된 요청 (id 파라미터 누락)
- `500`: 내부 서버 에러

---

### 4. 이벤트 조회 (`GET /terminal/events`)

특정 블록 범위 내의 거래 이벤트를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `fromBlock` | number | O | 시작 블록 번호 (포함) |
| `toBlock` | number | O | 종료 블록 번호 (포함) |

#### 제약 조건
- `fromBlock`은 `toBlock`보다 작거나 같아야 합니다
- 블록 범위는 최대 10,000 블록을 초과할 수 없습니다

#### 응답
```json
{
  "events": [
    {
      "eventType": "swap",
      "block": {
        "blockNumber": 12345678,
        "blockTimestamp": 1706500000
      },
      "txnId": "0x9876543210abcdef...",
      "txnIndex": 5,
      "eventIndex": 0,
      "maker": "0xaaaaaaaaaaaaaaa...",
      "pairId": "0xbbbbbbbbbbbbbb...",
      "asset0In": "1000000000000000000",
      "asset1Out": "500000000000000000000",
      "priceNative": "0.002",
      "reserves": {
        "asset0": "100000000000000000000",
        "asset1": "50000000000000000000000"
      }
    },
    {
      "eventType": "join",
      "block": {
        "blockNumber": 12345679,
        "blockTimestamp": 1706500012
      },
      "txnId": "0xfedcba0987654321...",
      "txnIndex": 10,
      "eventIndex": 1,
      "maker": "0xcccccccccccccc...",
      "pairId": "0xbbbbbbbbbbbbbb...",
      "amount0": "10000000000000000000",
      "amount1": "5000000000000000000000",
      "reserves": {
        "asset0": "110000000000000000000",
        "asset1": "55000000000000000000000"
      }
    }
  ]
}
```

#### 이벤트 타입

| 타입 | 설명 |
|------|------|
| `swap` | 토큰 스왑 이벤트 |
| `join` | 유동성 추가 이벤트 |
| `exit` | 유동성 제거 이벤트 |

#### Swap 이벤트 필드

| 필드 | 타입 | 설명 |
|------|------|------|
| `eventType` | string | "swap" |
| `block` | Block | 블록 정보 |
| `txnId` | string | 트랜잭션 해시 |
| `txnIndex` | number | 트랜잭션 인덱스 |
| `eventIndex` | number | 이벤트 인덱스 |
| `maker` | string | 거래 실행자 주소 |
| `pairId` | string | 거래쌍 주소 |
| `asset0In` | string? | asset0 입금량 (wei) |
| `asset1In` | string? | asset1 입금량 (wei) |
| `asset0Out` | string? | asset0 출금량 (wei) |
| `asset1Out` | string? | asset1 출금량 (wei) |
| `priceNative` | string | 네이티브 토큰 기준 가격 |
| `reserves` | Reserves | 거래 후 리저브 상태 |

#### Join/Exit 이벤트 필드

| 필드 | 타입 | 설명 |
|------|------|------|
| `eventType` | string | "join" 또는 "exit" |
| `block` | Block | 블록 정보 |
| `txnId` | string | 트랜잭션 해시 |
| `txnIndex` | number | 트랜잭션 인덱스 |
| `eventIndex` | number | 이벤트 인덱스 |
| `maker` | string | 유동성 공급자/제거자 주소 |
| `pairId` | string | 거래쌍 주소 |
| `amount0` | string | asset0 수량 (wei) |
| `amount1` | string | asset1 수량 (wei) |
| `reserves` | Reserves | 이벤트 후 리저브 상태 |

#### 에러 응답
- `400`: 잘못된 요청 (파라미터 누락, 블록 범위 초과, fromBlock > toBlock)
- `500`: 내부 서버 에러

---

### 5. 토큰 메타데이터 조회 (`GET /terminal/:token_address`)

토큰의 메타데이터 정보를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `token_address` | string | O | 토큰 컨트랙트 주소 (EVM 형식) |

#### 응답
```json
{
  "image": "https://storage.nadapp.net/images/token.png",
  "description": "A decentralized token for...",
  "website": "https://mytoken.com",
  "twitter": "https://x.com/mytoken",
  "telegram": "https://t.me/mytoken"
}
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `image` | string | 토큰 이미지 URL |
| `description` | string | 토큰 설명 |
| `website` | string | 웹사이트 URL |
| `twitter` | string? | X (Twitter) URL |
| `telegram` | string? | Telegram URL |

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 토큰 주소 형식)
- `404`: 토큰을 찾을 수 없음
- `500`: 내부 서버 에러

---

## TypeScript Interfaces

### 요청 타입

```typescript
// GET /terminal/asset
interface AssetQuery {
  id: string;  // Asset ID (토큰 컨트랙트 주소)
}

// GET /terminal/pair
interface PairQuery {
  id: string;  // Pair ID (거래쌍 컨트랙트 주소)
}

// GET /terminal/events
interface EventsQuery {
  fromBlock: number;  // 시작 블록 번호 (포함)
  toBlock: number;    // 종료 블록 번호 (포함), 최대 범위 10,000 블록
}
```

### 응답 타입

```typescript
// GET /terminal/latest-block
interface LatestBlockResponse {
  block: Block;
}

interface Block {
  blockNumber: number;
  blockTimestamp: number;
}

// GET /terminal/asset
interface AssetResponse {
  asset: Asset;
}

interface Asset {
  id: string;
  name: string;
  symbol: string;
  decimals: number;
  totalSupply?: string;
  circulatingSupply?: string;
  coinGeckoId?: string;
}

// GET /terminal/pair
interface PairResponse {
  pair: Pair;
}

interface Pair {
  id: string;
  dexKey: string;
  asset0Id: string;
  asset1Id: string;
  createdAtBlockNumber?: number;
  createdAtBlockTimestamp?: number;
  createdAtTxnId?: string;
  creator?: string;
  feeBps?: number;
}

// GET /terminal/events
interface EventsResponse {
  events: Event[];
}

interface Reserves {
  asset0: string;
  asset1: string;
}

type Event = SwapEvent | JoinEvent | ExitEvent;

interface SwapEvent {
  eventType: "swap";
  block: Block;
  txnId: string;
  txnIndex: number;
  eventIndex: number;
  maker: string;
  pairId: string;
  asset0In?: string;
  asset1In?: string;
  asset0Out?: string;
  asset1Out?: string;
  priceNative: string;
  reserves: Reserves;
}

interface JoinEvent {
  eventType: "join";
  block: Block;
  txnId: string;
  txnIndex: number;
  eventIndex: number;
  maker: string;
  pairId: string;
  amount0: string;
  amount1: string;
  reserves: Reserves;
}

interface ExitEvent {
  eventType: "exit";
  block: Block;
  txnId: string;
  txnIndex: number;
  eventIndex: number;
  maker: string;
  pairId: string;
  amount0: string;
  amount1: string;
  reserves: Reserves;
}

// GET /terminal/:token_address
interface TerminalMetadataResponse {
  image: string;
  description: string;
  website: string;
  twitter?: string;
  telegram?: string;
}
```

---

## 사용 예시

### 최신 블록 조회
```bash
curl -X GET "https://api.nadapp.net/terminal/latest-block"
```

### Asset 조회
```bash
curl -X GET "https://api.nadapp.net/terminal/asset?id=0x1234567890abcdef..."
```

### Pair 조회
```bash
curl -X GET "https://api.nadapp.net/terminal/pair?id=0xabcdef1234567890..."
```

### 이벤트 조회
```bash
curl -X GET "https://api.nadapp.net/terminal/events?fromBlock=12345000&toBlock=12346000"
```

### 토큰 메타데이터 조회
```bash
curl -X GET "https://api.nadapp.net/terminal/0x1234567890abcdef..."
```
