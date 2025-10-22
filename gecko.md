# GeckoTerminal Integration API 표준

**v0.1 / 2024년 11월**

GeckoTerminal Integration API 표준은 GeckoTerminal이 모든 파트너 탈중앙화 거래소(DEX)의 과거 및 실시간 데이터를 추적할 수 있도록 하는 HTTP 엔드포인트 세트입니다. API 제공자는 정확하고 최신 데이터를 제공할 책임이 있으며, GeckoTerminal은 모든 데이터 수집, 처리 및 제공을 담당합니다.

---

## 개요

GeckoTerminal 인덱서는 API 엔드포인트를 쿼리하여 사용 가능한 이벤트를 한 번에 하나 또는 여러 블록 단위로 지속적으로 인덱싱합니다. 각 블록은 한 번만 쿼리되므로, 반환된 모든 데이터가 인덱싱 시점에 정확한지 주의해야 합니다.

- API는 파트너가 배포, 제공 및 유지 관리해야 합니다
- API 엔드포인트에 도달할 수 없으면 인덱싱이 중단되며, 다시 사용 가능해지면 자동으로 재개됩니다
- 인덱서는 API 엔드포인트가 과부하되지 않도록 사용자 정의 가능한 속도 제한 및 블록 청크 크기를 허용합니다

### 엔드포인트 사용 방식

아래 스키마 섹션에서 각 엔드포인트에 대해 예상되는 스키마에 대한 자세한 설명이 제공됩니다.

**중요:**

- 금액 및 가격의 숫자는 number와 string 모두 가능합니다. 문자열은 JSON 숫자로 정확하게 직렬화할 수 없는 극도로 작거나 큰 숫자를 다룰 때 더 적합합니다.
- 스키마가 유효하지 않거나 예상치 못한 값을 포함하면 인덱싱이 중단됩니다 (예: swapEvent.priceNative=0 또는 pair.name="")

---

## 엔드포인트

### 1. Latest Block (최신 블록)

**요청:** `GET /latest-block`

**응답 스키마:**

```typescript
interface LatestBlockResponse {
  block: Block;
}
```

**응답 예시:**

```json
{
  "block": {
    "blockNumber": 100,
    "blockTimestamp": 1698126147
  }
}
```

**참고사항:**

- `/latest-block` 엔드포인트는 `/events` 엔드포인트와 동기화되어야 하며, `/events`에서 데이터를 사용할 수 있는 최신 블록만 반환해야 합니다
- 이벤트가 있는 최신 블록을 반환해야 한다는 의미는 아니지만, `/events`에서 아직 데이터를 사용할 수 없는 블록을 반환해서는 안 됩니다
- `/events`가 온디맨드로 데이터를 가져오는 경우에는 문제가 없지만, 백엔드에 인덱싱되고 지속된 데이터에 의존하는 경우 `/latest-block`은 최신 지속 블록을 인식해야 합니다
- 라이브 인덱싱 중에 인덱서는 `/latest-block`을 지속적으로 폴링하고 해당 데이터를 사용하여 `/events`를 쿼리합니다

---

### 2. Asset (자산)

**요청:** `GET /asset?id=:string`

**응답 스키마:**

```typescript
interface AssetResponse {
  asset: Asset;
}
```

**응답 예시:**

```json
{
  "asset": {
    "id": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
    "name": "Wrapped Ether",
    "symbol": "WETH",
    "decimals": 8,
    "totalSupply": 10000000,
    "circulatingSupply": 900000,
    "coinGeckoId": "ether"
  }
}
```

---

### 3. Pair (페어)

**요청:** `GET /pair?id=:string`

**응답 스키마:**

```typescript
interface PairResponse {
  pair: Pair;
}
```

**응답 예시:**

```json
{
  "pair": {
    "id": "0x11b815efB8f581194ae79006d24E0d814B7697F6",
    "dexKey": "uniswap",
    "asset0Id": "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2",
    "asset1Id": "0xdAC17F958D2ee523a2206206994597C13D831ec7",
    "createdAtBlockNumber": 100,
    "createdAtBlockTimestamp": 1698126147,
    "createdAtTxnId": "0xe9e91f1ee4b56c0df2e9f06c2b8c27c6076195a88a7b8537ba8313d80e6f124e",
    "feeBps": 100
  }
}
```

---

### 4. Events (이벤트)

**요청:** `GET /events?fromBlock=:number&toBlock=:number`

**응답 스키마:**

```typescript
interface EventsResponse {
  events: Array<{ block: Block } & (SwapEvent | JoinExitEvent)>;
}
```

**응답 예시:**

```json
{
  "events": [
    {
      "block": {
        "blockNumber": 10,
        "blockTimestamp": 1673319600
      },
      "eventType": "join",
      "txnId": "0xea1093d492a1dcb1bef708f771a99a96ff05dcab81ca76c31940300177fcf49f",
      "txnIndex": 0,
      "eventIndex": 0,
      "maker": "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045",
      "pairId": "0x456",
      "amount0": 10,
      "amount1": 5,
      "reserves": {
        "asset0": 100,
        "asset1": 50
      }
    },
    {
      "block": {
        "blockNumber": 11,
        "blockTimestamp": 1673406000
      },
      "eventType": "swap",
      "txnId": "0xea1093d492a1dcb1bef708f771a99a96ff05dcab81ca76c31940300177fcf49f",
      "txnIndex": 1,
      "eventIndex": 20,
      "maker": "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045",
      "pairId": "0x456",
      "asset0In": 0.0123456789,
      "asset1Out": 0.000123456789,
      "priceNative": 0.00000012345,
      "reserves": {
        "asset0": 0.0001,
        "asset1": 0.000000000000001
      }
    }
  ]
}
```

**참고사항:**

- `fromBlock`과 `toBlock`은 모두 포함됩니다: `/events?fromBlock=10&toBlock=15` 요청은 블록 10, 11, 12, 13, 14, 15의 모든 사용 가능한 이벤트를 포함해야 합니다

---

## 스키마

### Block (블록)

```typescript
interface Block {
  blockNumber: number;
  blockTimestamp: number;
  metadata?: Record<string, string>;
}
```

- `blockTimestamp`는 밀리초를 포함하지 않는 UNIX 타임스탬프여야 합니다
- `metadata`는 기본 스키마에서 다루지 않는 선택적 보조 정보를 포함하며 대부분의 경우 필요하지 않습니다

---

### Asset (자산)

```typescript
interface Asset {
  id: string;
  name: string;
  symbol: string;
  decimals: number;
  totalSupply?: string | number;
  circulatingSupply?: string | number;
  coinGeckoId?: string;
  metadata?: Record<string, string>;
}
```

- 대부분의 경우 자산 ID는 컨트랙트 주소에 해당합니다. ID는 대소문자를 구분하며, EVM 호환 블록체인의 경우 체크섬 주소 사용을 강력히 권장합니다
- `id`를 제외한 모든 Asset 속성은 변경 가능합니다. 인덱서는 최신 정보를 얻기 위해 주기적으로 자산을 쿼리합니다
- `totalSupply`는 선택 사항이지만, 제공되지 않으면 GeckoTerminal이 FDV/시가총액을 계산할 수 없습니다. 십진수 값 제공 (supply / (10 \*\* assetDecimals))
- `circulatingSupply`는 선택 사항이지만, 제공되지 않으면 정확한 시가총액을 표시하지 못할 수 있습니다. 십진수 값 제공 (supply / (10 \*\* assetDecimals))
- `coinGeckoId`는 선택 사항이지만 이미지, 설명 및 자체 보고/오프체인 순환 공급량과 같은 추가 토큰 정보를 표시하는 데 사용될 수 있습니다

---

### Pair (페어)

```typescript
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
  pool?: {
    id: string;
    name: string;
    assetIds: string[];
    pairIds: string[];
    metadata?: Record<string, string>;
  };
  metadata?: Record<string, string>;
}
```

**중요:**

- 모든 Pair 속성은 불변입니다 - 인덱서는 특정 페어를 한 번만 쿼리합니다
- 대부분의 경우 페어 ID는 컨트랙트 주소에 해당합니다. ID는 대소문자를 구분하며, EVM 호환 블록체인의 경우 체크섬 주소 사용을 강력히 권장합니다
- `dexKey`는 이 페어를 호스팅하는 DEX의 식별자입니다. 대부분의 경우 uniswap과 같은 정적 값이지만, 여러 DEX를 추적하는 경우 팩토리 주소와 같은 ID를 사용할 수 있습니다
- **asset0과 asset1 순서는 절대 변경되어서는 안 됩니다.** amount0/reserve0은 항상 동일한 asset0을 참조하고, amount1/reserve1은 항상 동일한 asset1을 참조합니다. 자산 순서가 변경되면 변경 후 모든 데이터가 유효하지 않게 되며 재인덱싱이 필요합니다
- 이를 확인하는 간단한 전략은 가능한 경우 온체인 순서를 따르는 것입니다 (예: 풀에 token0과 token1이 있는 경우 각각 asset0과 asset1에 매핑). 그렇지 않으면 자산을 알파벳순으로 정렬합니다. 예: 0xAAA와 0xZZZ 자산을 포함하는 페어에서 asset0=0xAAA, asset1=0xZZZ
- GeckoTerminal UI는 필요에 따라 페어를 자동으로 반전하고 가장 논리적인 순서로 기본 설정합니다 (예: USD/BTC가 아닌 BTC/USD)
- `feeBps`는 스왑 수수료를 bps 단위로 나타냅니다. 예를 들어, 1% 수수료는 feeBps=100에 해당합니다
- `pool`은 다중 자산 풀을 지원하는 DEX에 권장되며, GeckoTerminal UI가 동일한 다중 자산 풀의 여러 페어를 연관시킬 수 있게 합니다

---

### Swap Event (스왑 이벤트)

```typescript
interface SwapEvent {
  eventType: "swap";
  txnId: string;
  txnIndex: number;
  eventIndex: number;
  maker: string;
  pairId: string;
  asset0In?: number | string;
  asset1In?: number | string;
  asset0Out?: number | string;
  asset1Out?: number | string;
  priceNative: number | string;
  reserves: {
    asset0: number | string;
    asset1: number | string;
  };
  metadata?: Record<string, string>;
}
```

**설명:**

- `txnId`는 트랜잭션 해시와 같은 트랜잭션 식별자입니다
- `txnIndex`는 블록 내 트랜잭션의 순서를 나타내며, 인덱스가 높을수록 블록에서 나중에 처리된 것입니다
- `eventIndex`는 트랜잭션 내 이벤트의 순서를 나타내며, 인덱스가 높을수록 트랜잭션에서 나중에 처리된 것입니다
- **txnIndex + eventIndex의 조합은 다른 이벤트 유형을 포함하여 블록 내의 특정 이벤트에 대해 고유해야 합니다**
- `maker`는 트랜잭션 제출을 담당하는 계정의 식별자입니다. 대부분의 경우 트랜잭션은 토큰을 보내거나 받은 계정과 동일한 계정에서 제출되지만, 이것이 적용되지 않는 경우 (예: 애그리게이터가 제출한 트랜잭션) 기본 계정을 식별하도록 노력해야 합니다
- 모든 금액(assetIn/assetOut/reserve)은 십진수화되어야 합니다 (amount / (10 \*\* assetDecimals))
- Reserves는 스왑 이벤트가 발생한 후 각 자산의 풀링된 금액을 나타냅니다
- **asset0In + asset1Out 또는 asset1In + asset0Out의 조합이 예상됩니다.** 여러 자산이 들어오거나 나가는 경우 스왑 이벤트는 유효하지 않은 것으로 간주되며 인덱싱이 중단됩니다
- `priceNative`는 해당 이벤트에서 asset1로 표시된 asset0의 가격을 나타냅니다
  - priceNative = price (asset1) / price (asset0)
  - 예: 이론적인 BTC/USD 페어에서 priceNative는 30000입니다 (1 BTC = 30000 USD)
  - 마찬가지로 이론적인 USD/BTC 페어에서 priceNative는 0.00003333333333입니다 (1 USD = 0.00003333333333 BTC)
- 인덱서는 금액/가격/리저브에 대해 최대 50자리 소수점을 사용하며, 이후의 모든 소수점 이하는 무시됩니다. 정확한 가격을 보장하기 위해 50자리 소수점을 모두 사용하는 것이 강력히 권장됩니다

---

### Join/Exit Event (조인/이그짓 이벤트)

```typescript
interface JoinExitEvent {
  eventType: "join" | "exit";
  txnId: string;
  txnIndex: number;
  eventIndex: number;
  maker: string;
  pairId: string;
  amount0: number | string;
  amount1: number | string;
  reserves: {
    asset0: number | string;
    asset1: number | string;
  };
  metadata?: Record<string, string>;
}
```

**설명:**

- `txnId`는 트랜잭션 해시와 같은 트랜잭션 식별자입니다
- `txnIndex`는 블록 내 트랜잭션의 순서를 나타냅니다
- `eventIndex`는 트랜잭션 내 이벤트의 순서를 나타냅니다
- txnIndex + eventIndex의 조합은 다른 이벤트 유형을 포함하여 블록 내의 특정 이벤트에 대해 고유해야 합니다
- `maker`는 트랜잭션 제출을 담당하는 계정의 식별자입니다
- 모든 금액은 십진수화되어야 합니다 (amount / (10 \*\* assetDecimals))
- Reserves는 조인/이그짓 이벤트가 발생한 후 각 자산의 풀링된 금액을 나타냅니다

---

## FAQ

### /events 엔드포인트를 얼마나 자주 호출하나요?

기본적으로 2초마다 호출합니다. 하지만 이 값을 조정할 수도 있습니다. API가 필요한 속도 제한을 제공하는지 확인하세요.

### 이벤트 가져오기는 어떻게 작동하나요?

1. 먼저 `/latestblock`을 호출하여 최신 블록 번호를 가져옵니다 (인덱싱된 블록)
2. 그런 다음 `/events`를 호출하고 `?fromBlock = 마지막으로 동기화된 블록`과 `?toBlock = 위 호출에서 가져온 최신 블록`을 전달합니다
3. 예: 마지막으로 동기화된 블록이 100입니다. `/latestblock`을 호출하면 105가 반환됩니다. 그런 다음 `/events?fromBlock=100&toBlock=105`를 호출하여 이 범위 내의 이벤트를 검색하고 인덱싱합니다.

### txnIndex와 eventIndex는 어떻게 사용되나요? 멀티홉 스왑의 경우 동일한 txnIndex를 갖게 되는데요?

txnIndex와 eventIndex의 조합을 사용하여 고유한 이벤트 시퀀스를 결정하고 그에 따라 인덱싱합니다. 이는 유동성 및 가격 계산이 언제든지 풀의 토큰 수를 추적하는 데 중요합니다. 멀티홉 스왑의 경우에도 스왑은 순차적으로 발생하므로 동일한 txnIndex를 가질 수 있지만 eventIndex는 고유해야 합니다.

### Join/Exit 이벤트란 무엇인가요?

풀링된 토큰의 변경, 즉 리저브를 나타냅니다 (유동성 추가/제거). 이를 제공할 수 없는 경우, Swap 이벤트 엔드포인트 응답에 리저브 데이터가 있는지 확인하세요.

### Asset의 id와 name은 무엇인가요?

`id`는 토큰 컨트랙트 주소입니다. `name`은 토큰 이름입니다.

### 일부 풀의 feeBps가 변경될 수 있다면?

안타깝게도 현재 feeBps 변경을 지원하지 않습니다.

### 어떤 컨트랙트 주소 형식을 제공해야 하나요?

새로운 DEX를 통합할 때 각 블록 탐색기에 따라 컨트랙트 주소 형식을 표준화하는 것이 중요합니다. 이렇게 하면 CoinGecko의 자동 코인 마켓 매핑(CMM)이 올바르게 작동합니다. 컨트랙트 주소가 탐색기 형식과 다르면 (예: 체크섬 누락 또는 대소문자 불일치) CoinGecko와 GeckoTerminal 모두에서 매핑 실패 또는 부정확한 데이터 표시가 발생할 수 있습니다.

### DEX 애그리게이터를 지원하나요?

DEX 애그리게이터는 지원하지 않습니다. 애그리게이터를 통해 라우팅되는 모든 스왑은 기본 DEX에 귀속됩니다.
