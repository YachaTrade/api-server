# Terminal API 문서

## 개요

Terminal API는 GeckoTerminal과 같은 외부 가격 집계 서비스와의 연동을 위한 API입니다.

- **최신 블록 조회**: 현재 인덱싱된 최신 블록 정보 반환
- **Asset 조회**: 토큰(Asset) 정보 조회
- **Pair 조회**: 거래쌍 정보 조회
- **이벤트 조회**: 특정 블록 범위 내의 거래 이벤트 조회
- **메타데이터 조회**: 토큰의 메타데이터 조회

> **경로 안내**: 모든 Terminal 엔드포인트는 **루트 경로**에 마운트됩니다 (`/latest-block`, `/asset`, `/pair`, `/events`, `/{token_address}`). `/terminal/*` prefix는 사용하지 않습니다.

> **시장 노출 정책**: 시장 데이터 엔드포인트인 `/pair`와 `/events`는 `DEX`, `V2_DEX`만 제공합니다. `CURVE`, `V2_CURVE` 및 알 수 없는 시장 유형은 노출하지 않습니다. `/asset`, `/{token_address}`, `/latest-block`의 동작은 시장 유형과 무관하며 변경되지 않습니다.

---

## V2 라우팅 / 가격 산정 요약

V2(멀티 quote bonding curve + NadSwap DEX)부터 `/pair`와 `/events`의 값은 토큰의 `market.quote_id`(quote 자산: WMON 또는 LVMON 등) 기준으로 산정됩니다. **응답 스키마(필드명·JSON 키)는 V1과 동일**하며, 값(dexKey·feeBps·asset 정렬·priceNative)만 quote 기준으로 바뀌는 **동작(behavioral) 변경**입니다.

### dexKey 매핑

시장 데이터 응답에 노출되는 `market_type`에 따라 거래 venue 식별자(`dexKey`)가 결정됩니다.

| market_type | dexKey | 설명 |
|---|---|---|
| `DEX` (V1) | `capricorn` | V1 졸업 DEX (Capricorn) |
| `V2_DEX` | `nadswap` | V2 졸업 DEX (NadSwap 페어) |

`CURVE`, `V2_CURVE` 및 알 수 없는 `market_type`은 응답 전에 제외되므로 `dexKey`가 노출되지 않습니다.

### feeBps 산식

`feeBps`는 거래 수수료를 basis point(1bp = 0.01%)로 나타냅니다.

| market_type | feeBps | 근거 |
|---|---|---|
| `DEX` (V1) | `100` 고정 (1%) | V1 고정 수수료 |
| `V2_DEX` | `25 + creator_fee_rate + dex_protocol_fee_rate` | `25`는 NadSwap LP 수수료(0.25%, 고정 상수) + `fee_config` 합산 |

- V2_DEX 토큰에 `fee_config` 행이 없으면(인덱서가 Setup 이벤트 미수신) `feeBps` 필드는 **생략**됩니다 (틀린 `0%` 또는 부분값 노출 방지). V1 DEX 토큰은 항상 `100`을 반환합니다.

### Asset 정렬 / 가격 단위

- `asset0Id` / `asset1Id`는 **토큰 주소 vs quote 주소를 소문자 오름차순 비교**하여 정렬합니다 (GeckoTerminal의 알파벳 페어링 규칙). 더 작은 쪽이 `asset0`.
- `priceNative`는 **`asset0`를 `asset1` 단위로 표현한 가격**(`= amount(asset1) / amount(asset0)`)이며, 같은 이벤트의 `reserves.asset1 / reserves.asset0`과 방향이 일치합니다. 정렬에 따라 token이 `asset0`이면 `quote/token`, quote가 `asset0`이면 `token/quote`가 됩니다. GeckoTerminal은 `asset0Id` / `asset1Id` + reserves로 각 토큰 가격을 역산하므로, 이 값은 페어 상대(asset0-relative) 가격입니다 (특정 토큰의 quote 단위 고정 가격이 아님).
- swap의 `reserves.asset0` / `reserves.asset1`도 동일한 정렬 규칙을 따릅니다.

### pairId 매핑

| market_type | pairId |
|---|---|
| `DEX` / `V2_DEX` | 풀(pool) 주소 |

`/pair`와 `/events`에 노출되는 pairId는 DEX 풀 주소입니다. `/events`의 swap은 **각 이벤트 자신의 `market_type`** 으로 노출 여부를 판단하고, join/exit는 이벤트의 `market_id`를 pairId로 사용합니다. Curve 및 알 수 없는 시장 이벤트는 제외되므로 본딩 커브 주소가 pairId로 노출되지 않습니다.

### Decimals 가정

본 API의 적용 범위는 nadfun 라이프사이클 토큰(V2_CURVE → 졸업 → V2_DEX)입니다. 토큰과 quote 자산 **둘 다 18 decimals**로 가정하며, 모든 amount/reserve는 `10^18`로 나눠 십진화됩니다. 외부 DEX 상장 토큰(임의 token0/token1, 비-18 decimals)은 범위에서 제외됩니다.

---

## API 엔드포인트

### 1. 최신 블록 조회 (`GET /latest-block`)

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

### 2. Asset 조회 (`GET /asset`)

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
    "totalSupply": "1000000000",
    "circulatingSupply": "800000000",
    "coinGeckoId": "monad"
  }
}
```

#### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `asset.id` | string | 토큰 컨트랙트 주소 |
| `asset.name` | string | 토큰 이름 |
| `asset.symbol` | string | 토큰 심볼 |
| `asset.decimals` | number | 소수점 자릿수 (18 고정) |
| `asset.totalSupply` | string? | 총 공급량 (십진화된 값, 항상 `1000000000`) |
| `asset.circulatingSupply` | string? | 유통 공급량 (십진화된 값, `10^18`로 나눈 후) |
| `asset.coinGeckoId` | string? | CoinGecko ID (`monad` 고정) |

#### 에러 응답
- `400`: 잘못된 요청 (id 파라미터 누락)
- `500`: 내부 서버 에러

---

### 3. Pair 조회 (`GET /pair`)

거래쌍(Pair) 정보를 조회합니다. 쿼리 `id`는 **pair ID(= DEX pool 주소, `/events`가 내리는 `pairId`와 동일)**이며, 해당 pool의 `market_type`이 `DEX` 또는 `V2_DEX`인 경우에만 페어를 반환합니다. `CURVE`, `V2_CURVE` 또는 알 수 없는 시장 유형은 `404`로 처리됩니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요

#### Query Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `id` | string | O | 토큰 컨트랙트 주소 |

#### 응답 (V2_DEX 예시, quote = LVMON)
```json
{
  "pair": {
    "id": "0xabcdef1234567890...",
    "dexKey": "nadswap",
    "asset0Id": "0xBe3fa50514D9617ce645a02B34F595541AF02b6b",
    "asset1Id": "0xff00000000000000000000000000000000000000",
    "createdAtBlockNumber": 12345000,
    "createdAtBlockTimestamp": 1706400000,
    "createdAtTxnId": "0x9876543210abcdef...",
    "creator": "0xcccccccccccccccc...",
    "feeBps": 555
  }
}
```

> 위 예시는 V2_DEX(`nadswap`) 페어로, quote가 LVMON(`0xBe3f...`)이고 토큰 주소(`0xff00...`)가 더 큰 경우입니다. 소문자 비교에서 LVMON이 작으므로 LVMON이 `asset0`이 됩니다. `feeBps` `555`는 예시값(`25` LP + creator + dex)입니다.

#### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `pair.id` | string | DEX/V2_DEX 풀 주소 (위 pairId 매핑 표 참고) |
| `pair.dexKey` | string | DEX 식별자 (`capricorn` 또는 `nadswap`) |
| `pair.asset0Id` | string | 토큰·quote 중 소문자 주소가 더 작은 쪽 |
| `pair.asset1Id` | string | 토큰·quote 중 소문자 주소가 더 큰 쪽 |
| `pair.createdAtBlockNumber` | number? | 생성 블록 번호 |
| `pair.createdAtBlockTimestamp` | number? | 생성 시간 (Unix timestamp) |
| `pair.createdAtTxnId` | string? | 생성 트랜잭션 해시 |
| `pair.creator` | string? | 생성자 주소 |
| `pair.feeBps` | number? | 거래 수수료 (basis points). V2_DEX 토큰에 `fee_config`가 없으면 생략됨 (feeBps 산식 표 참고) |

#### 에러 응답
- `400`: 잘못된 요청 (id 파라미터 누락)
- `404`: 페어를 찾을 수 없음
- `500`: 내부 서버 에러

---

### 4. 이벤트 조회 (`GET /events`)

특정 블록 범위 내의 DEX 거래 이벤트(swap / join / exit)를 조회합니다. swap은 이벤트 자신의 `market_type`, join/exit는 연결된 현재 market을 기준으로 `DEX`, `V2_DEX`만 반환하며 Curve 및 알 수 없는 시장 유형은 제외합니다.

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

#### 응답 (V2_DEX swap + join 예시, quote = LVMON)
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
      "pairId": "0xabcdef1234567890...",
      "asset0In": "1",
      "asset1Out": "2",
      "priceNative": "2",
      "reserves": {
        "asset0": "100",
        "asset1": "200"
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
      "pairId": "0xabcdef1234567890...",
      "amount0": "10",
      "amount1": "20",
      "reserves": {
        "asset0": "110",
        "asset1": "220"
      }
    }
  ]
}
```

> swap 예시: quote(LVMON)가 `asset0`이고 매수(buy)이므로 quote가 들어가고(`asset0In`) 토큰이 나옵니다(`asset1Out`). `priceNative`(`2`)는 `asset0`(LVMON)를 `asset1`(token) 단위로 표현한 값 = `amount(asset1)/amount(asset0)` = `token/quote`이며, `reserves.asset1 / reserves.asset0`(`200/100 = 2`)과 일치합니다 (특정 토큰의 quote 단위 가격이 아님). amount/reserve는 모두 `10^18`로 나눈 십진값입니다.

#### 이벤트 타입

| 타입 | 설명 |
|------|------|
| `swap` | 토큰 스왑 이벤트 |
| `join` | 유동성 추가 이벤트 (LP add) |
| `exit` | 유동성 제거 이벤트 (LP remove) |

#### Swap 이벤트 필드

| 필드 | 타입 | 설명 |
|------|------|------|
| `eventType` | string | "swap" |
| `block` | Block | 블록 정보 |
| `txnId` | string | 트랜잭션 해시 |
| `txnIndex` | number | 트랜잭션 인덱스 |
| `eventIndex` | number | 이벤트 인덱스 (log index) |
| `maker` | string | 거래 실행자 주소 |
| `pairId` | string | DEX/V2_DEX 이벤트의 풀 주소. 필터 기준은 이벤트 자신의 `market_type` |
| `asset0In` | string? | asset0 입금량 (십진화된 값) |
| `asset1In` | string? | asset1 입금량 (십진화된 값) |
| `asset0Out` | string? | asset0 출금량 (십진화된 값) |
| `asset1Out` | string? | asset1 출금량 (십진화된 값) |
| `priceNative` | string | `asset0`를 `asset1` 단위로 표현한 가격 (`amount(asset1)/amount(asset0)`, `reserves.asset1/reserves.asset0`과 일치) |
| `reserves` | Reserves | 거래 후 리저브 상태 (asset 정렬 동일) |

#### Join/Exit 이벤트 필드

| 필드 | 타입 | 설명 |
|------|------|------|
| `eventType` | string | "join" 또는 "exit" |
| `block` | Block | 블록 정보 |
| `txnId` | string | 트랜잭션 해시 |
| `txnIndex` | number | 트랜잭션 인덱스 |
| `eventIndex` | number | 이벤트 인덱스 (log index) |
| `maker` | string | 유동성 공급자/제거자 주소 |
| `pairId` | string | 이벤트의 `market_id` (풀 주소) |
| `amount0` | string | asset0 수량 (십진화된 값) |
| `amount1` | string | asset1 수량 (십진화된 값) |
| `reserves` | Reserves | 이벤트 후 리저브 상태 |

#### 에러 응답
- `400`: 잘못된 요청 (파라미터 누락, 블록 범위 초과, fromBlock > toBlock)
- `500`: 내부 서버 에러

---

### 5. 토큰 메타데이터 조회 (`GET /{token_address}`)

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

## 알려진 한계

1. **졸업 초기 유동성 시딩 미노출**: 커브 → 풀로 졸업할 때의 초기 유동성 민팅(curve → pool initial mint)은 pool-centric `dex_mint` 테이블에만 기록되므로 `/events`의 `join` 이벤트로 **나타나지 않습니다**. `/events`에는 사용자가 직접 수행한 LP add/remove(`mint` / `burn` 테이블)만 `join` / `exit`로 노출됩니다.
2. **`/pair`는 DEX 현재 페어만 반환**: 현재 시장이 `DEX` 또는 `V2_DEX`인 토큰의 현재 페어 하나만 반환하며 Curve 토큰은 `404`입니다.
3. **Curve 이벤트 미노출**: `/events`는 `DEX`, `V2_DEX` swap/join/exit만 반환하므로 졸업 전 Curve 거래 이력은 포함하지 않습니다.

---

## TypeScript Interfaces

> 아래 타입은 V1과 V2에서 **동일**합니다 (V2 변경은 값/동작에 국한). 응답값 의미는 위 "V2 라우팅 / 가격 산정 요약"을 참고하세요.

### 요청 타입

```typescript
// GET /asset
interface AssetQuery {
  id: string;  // Asset ID (토큰 컨트랙트 주소)
}

// GET /pair
interface PairQuery {
  id: string;  // pair ID (= DEX pool 주소, /events의 pairId)
}

// GET /events
interface EventsQuery {
  fromBlock: number;  // 시작 블록 번호 (포함)
  toBlock: number;    // 종료 블록 번호 (포함), 최대 범위 10,000 블록
}
```

### 응답 타입

```typescript
// GET /latest-block
interface LatestBlockResponse {
  block: Block;
}

interface Block {
  blockNumber: number;
  blockTimestamp: number;
}

// GET /asset
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

// GET /pair
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

// GET /events
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

// GET /{token_address}
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
curl -X GET "https://api.nadapp.net/latest-block"
```

### Asset 조회
```bash
curl -X GET "https://api.nadapp.net/asset?id=0x1234567890abcdef..."
```

### Pair 조회
```bash
curl -X GET "https://api.nadapp.net/pair?id=0x1234567890abcdef..."
```

### 이벤트 조회
```bash
curl -X GET "https://api.nadapp.net/events?fromBlock=12345000&toBlock=12346000"
```

### 토큰 메타데이터 조회
```bash
curl -X GET "https://api.nadapp.net/0x1234567890abcdef..."
```
