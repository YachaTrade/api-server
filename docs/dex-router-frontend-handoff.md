# Dex 라우터 — 프론트엔드 레퍼런스 / 핸드오프

> 대상: nadfun.com Frontend
> 기준 브랜치: `v2`
> 범위: `/dex/*` 전체 엔드포인트의 현행 계약. 마지막 주요 변경 — **`/dex/search` 제거**(→ `/dex/tokens?q=`), **`/dex/reserves` 추가**, 풀/포지션 토큰 메타데이터 **whitelist 우선 소싱**.

---

## TL;DR (최근 변경)

| 변경 | 내용 |
|------|------|
| ❌ **제거** | `GET /dex/search` — 호출 시 **404**. 검색은 `GET /dex/tokens?q=` 로 통합 |
| ➕ **추가** | `GET /dex/reserves?pool_id=` — 온체인 `getReserves()` 드롭인(RPC 없음) |
| 🔁 **변경** | `/dex/pools`·`/dex/positions` 토큰 `symbol`/`image_uri`/`decimals` 가 **`whitelist_token` 우선** 소싱으로 전환 → 화이트리스트 토큰(USDT/WMON 등)은 큐레이션된 심볼·아이콘으로 노출 |

> **반드시 할 일:** 기존 `/dex/search` 호출 코드를 전부 `/dex/tokens?q=` 로 교체. 빠뜨리면 검색이 404로 깨집니다.

---

## 1. 엔드포인트 일람

| Method | Path | 인증 | 설명 | 상태 |
|--------|------|------|------|------|
| GET | `/dex/tokens` | 무인증 (`?account=` 옵션) | Select Token 리스트 + 검색(`?q=`) | 활성 (§3) |
| GET | `/dex/positions/{account_id}` | 무인증 | 지갑 LP 포지션 | 활성 (§4) |
| GET | `/dex/pools/{pool_id}` | 무인증 | 풀 상세 (Deposit/Withdraw UI) | 활성 (§5) |
| GET | `/dex/reserves?pool_id=` | 무인증 | 풀 리저브 (getReserves 드롭인) | 활성 (§6) |
| ~~GET~~ | ~~`/dex/search`~~ | ~~세션 필수~~ | ~~토큰 검색~~ | **제거됨** |

> 모든 `/dex/*` 는 **무인증**. 경로/쿼리의 EVM 주소는 **EIP-55 체크섬**으로 전달하세요(소문자 주소 금지). 잘못된 주소는 `400`.

---

## 2. 마이그레이션: `/dex/search` → `/dex/tokens?q=`

### 요청 비교

```diff
- GET /dex/search?q=chog&page=1&limit=50      # 세션 쿠키 필요
+ GET /dex/tokens?q=chog&account=0xabc...&page=1&limit=50   # 무인증, account는 옵션
```

### 프론트 예시

```ts
// Before
const res = await fetch(`/dex/search?q=${encodeURIComponent(term)}`, {
  credentials: 'include', // 세션 쿠키
});

// After
const params = new URLSearchParams({ q: term });
if (walletAddress) params.set('account', walletAddress); // EIP-55 체크섬 주소
const res = await fetch(`/dex/tokens?${params}`);
```

### 동작 차이 (구 `/dex/search` 대비)

| 항목 | 구 `/dex/search` | 신 `/dex/tokens?q=` |
|------|------------------|---------------------|
| 매칭 방식 | **substring** (`%q%`) | **prefix** (`q%`) — symbol/name 앞부분 일치 |
| external 토큰 | 검색 대상 포함 | **기본 제외**, **full CA 정확 입력 시에만** 노출 |
| 인증 | 세션 필수 | **무인증** (`?account=` 옵션) |
| 잔고 | 항상 첨부 | `?account=` 줄 때만 첨부 |
| 응답 타입 | `DexTokenEntry` | 동일 `DexTokenEntry` (필드는 §3 참고) |

> ⚠️ 매칭이 substring→prefix로 **좁아졌습니다.** "hog"로 "CHOG"를 찾던 동작은 더 이상 안 됩니다. "ch" 같은 앞글자로 검색하세요. 컨트랙트 주소로 임의 external 토큰을 찾으려면 **전체 주소(0x + 40 hex)** 를 정확히 넣어야 합니다.

---

## 3. `GET /dex/tokens` 전체 계약

### Query 파라미터

| 파라미터 | 타입 | 필수 | 기본 | 설명 |
|----------|------|------|------|------|
| `account` | string | 옵션 | — | **EIP-55 체크섬** 지갑 주소. 주면 각 토큰에 보유 잔고·USD 가치 첨부 |
| `q` | string | 옵션 | — | 검색어. **없으면** 4-tier 기본 리스트, **있으면** 검색 모드 (§아래) |
| `page` | int | 옵션 | `1` | 1-indexed 페이지 |
| `limit` | int | 옵션 | `50` | 페이지 크기, **최대 100** (초과 시 100으로 캡) |

### `q` 분류 규칙 (서버가 자동 판별)

| 입력 형태 | 분류 | 동작 |
|-----------|------|------|
| `q` 없음 / 빈 문자열 | 기본 리스트 | 화이트리스트 ∪ nadfun V2 4-tier 정렬 |
| `0x` + **정확히 40 hex** | **full CA** | `token∪dex_token∪quote_token∪whitelist`에서 1건 조회. DB에 없으면 **RPC 온체인 메타로 `external` 1건 fallback** |
| `0x` + 40 미만 hex | **partial CA** | `token_id` prefix — 검색 후보집합 `token∪dex_token∪quote_token∪whitelist` (**external·V1 포함**) |
| 그 외 텍스트 | **text** | `symbol`/`name` prefix(`q%`) — 동일 후보집합 (**external·V1 포함**) |

> 검색 후보집합은 기본 리스트(화이트리스트+nadfun V2)와 **다릅니다** — V1·external 토큰은 **검색에서만** 노출됩니다(기본 리스트엔 제외). 검색 시 보유(`balance>0`) 토큰은 타입과 무관하게 상단으로 정렬됩니다.

### 응답 형태

```jsonc
// 200 OK
{
  "tokens": [ /* DexTokenEntry[] */ ],
  "total_count": 12   // 페이지네이션 전 매칭 총 개수 (full CA 검색은 0 또는 1)
}
```

#### `DexTokenEntry` 필드

| 필드 | 타입 | 설명 |
|------|------|------|
| `token_id` | string | 토큰 컨트랙트 주소 (EIP-55 체크섬) |
| `symbol` | string | 심볼 |
| `name` | string | 토큰명 |
| `decimals` | int | 소수 자릿수 |
| `image_uri` | string | 로고 URL (없으면 빈 문자열) |
| `token_type` | string | `"whitelist"` \| `"nadfun_v2"` \| `"nadfun_v1"` \| `"external"`. 테이블 멤버십(whitelist_token / token.version V2·V1 / 그 외). **렌더 분기·external 판정(`=== "external"`)용** |
| `balance` | string \| null | **raw wei** 잔고(보유 시). 미보유 또는 `account` 미제공 시 `null` → **`balance != null` 로 보유 판정** |
| `balance_usd` | string \| null | 보유분 USD 가치. 미보유/미제공 시 `null` |
| `price_usd` | string \| null | 토큰 1개당 **USD 단가**. **`account` 무관**(보유 여부와 상관없이 항상 제공). 소수점 **8자리까지 truncate**. 가격 미상이면 `null`. (V2/external = `dex_token_price` 뷰(deepest-TVL 풀) 우선·없으면 market×quote fallback, whitelist = DefiLlama) |

> `balance`는 **raw 정수 문자열(×10^decimals)** 입니다. 표시할 땐 FE에서 `decimals`로 나누세요. (`balance` / `10^decimals`)
> `price_usd`는 이미 **사람이 읽는 USD 단가**(스케일 적용 완료)입니다. 스왑 금액의 USD 환산 = 입력수량 × `price_usd`.

### 정렬 (tier) — 기본 리스트일 때

`account` 제공 시 보유/미보유를 나눠 4-tier로 정렬됩니다:

| tier | 그룹 | 그룹 내 정렬 |
|------|------|--------------|
| 1 | 보유 × 화이트리스트 | `balance_usd` 내림차순 |
| 2 | 보유 × nadfun V2 | `balance_usd` 내림차순 |
| 3 | 미보유 화이트리스트 | 화이트리스트 고정 `sort_order` 오름차순 |
| 4 | 미보유 nadfun V2 | `market_cap_usd` 내림차순 |

`account` 미제공 시 tier 1·2는 비고 3·4만 나옵니다(=화이트리스트 먼저, 그다음 V2).

### 예시

**(a) 기본 리스트 + 지갑 연결**

```
GET /dex/tokens?account=0x000000000000000000000000000000000000aA11&page=1&limit=50
```
```jsonc
{
  "tokens": [
    { "token_id": "0x…cC01", "symbol": "MON",  "name": "Monad",  "decimals": 18,
      "image_uri": "https://…/mon.png", "token_type": "whitelist",
      "balance": "1500000000000000000", "balance_usd": "4.32", "price_usd": "2.88" },
    { "token_id": "0x…cC02", "symbol": "LVMON", "name": "…", "decimals": 18,
      "image_uri": "", "token_type": "whitelist", "balance": null, "balance_usd": null,
      "price_usd": null }
  ],
  "total_count": 8
}
```

**(b) 텍스트 검색 (무인증)**

```
GET /dex/tokens?q=ch
```
→ `symbol`/`name`이 `ch`로 시작하는 화이트리스트·V2 토큰만. external 제외, `balance`/`balance_usd`는 `null`.

**(c) external 토큰 직접 조회 (full CA)**

```
GET /dex/tokens?q=0x000000000000000000000000000000000000dEaD
```
→ 해당 주소 1건. external이면 `token_type: "external"`, `total_count: 1` (없으면 빈 배열, `total_count: 0`).

---

## 4. `GET /dex/positions/{account_id}`

지갑이 보유한 **열린 LP 포지션**(`lp_position.balance > 0`)을 풀별로 1건씩 반환. 포지션이 없으면 **200 + 빈 배열**(404 아님).

| 항목 | 값 |
|------|----|
| Path | `account_id` — 지갑 주소(EIP-55 체크섬) |
| 200 | `LpPositionsResponse` |
| 400 | 잘못된 account 주소 |

### 응답 형태

```jsonc
{
  "account_id": "0x…",            // 경로 파라미터 echo (EIP-55)
  "positions": [ /* LpPositionEntry[] */ ]   // pool_id 오름차순 정렬
}
```

#### `LpPositionEntry`

| 필드 | 타입 | 설명 |
|------|------|------|
| `pool_info` | `PoolInfo` | 풀 단위 정보 (§7) — `/dex/pools` 와 동일 형태 |
| `token0` | `LpPositionTokenSide` | 풀 인덱스 0 토큰 |
| `token1` | `LpPositionTokenSide` | 풀 인덱스 1 토큰 |
| `balance` | string | 지갑의 **LP 토큰** 잔고 (raw wei, `lp_in - lp_out`) |
| `liquidity_usd` | string \| null | **현재** 시가 유동성 USD = `balance × tvl_usd / total_supply`, 8자리 truncate. `total_supply=0`이면 `null` |

#### `LpPositionTokenSide`

| 필드 | 타입 | 설명 |
|------|------|------|
| `token_id` | string | 토큰 주소 (EIP-55) |
| `symbol` | string | 심볼 — **whitelist 우선** → `token`/`dex_token`/`quote_token`. 없으면 `""` |
| `decimals` | int | 소수 자릿수 (whitelist 우선, 기본 18) |
| `image_uri` | string | 아이콘 URL — **whitelist 우선** → registry. 없으면 `""` |
| `deposit_amount` | string | 입금 원가(raw wei). **입금 시점 고정**(mark-to-market 아님) |
| `deposit_usd` | string | 입금 시점(블록타임) USD 가치. **고정** |
| `current_amount` | string \| null | **지금** 전량 인출 시 받을 양 = `floor(balance × reserve / total_supply)` (**raw wei 정수**). 시가 반영. `total_supply=0`이면 `null` |

> `deposit_*` 는 **고정 원가**(입금 당시), `current_amount`/`liquidity_usd` 는 **현재 시가**입니다. PnL = 현재 시가 − 원가.

---

## 5. `GET /dex/pools/{pool_id}`

Deposit/Withdraw UI용 풀 상세: 리저브, TVL, 총 LP 공급, APR, fee_config 분해. 풀이 없으면 **404**.

| 항목 | 값 |
|------|----|
| Path | `pool_id` — 풀(페어) 주소(EIP-55 체크섬) |
| 200 | `PoolDetailResponse` |
| 400 | 잘못된 pool id / 404 풀 없음 |

### 응답 형태

```jsonc
{
  "pool_info":  { /* PoolInfo — §7 */ },
  "token0":     { /* PoolTokenSide */ },
  "token1":     { /* PoolTokenSide */ },
  "fee_config": { /* FeeConfigInfo | null */ }
}
```

#### `PoolTokenSide`

| 필드 | 타입 | 설명 |
|------|------|------|
| `token_id` | string | 토큰 주소 (EIP-55) |
| `symbol` | string | 심볼 — **whitelist 우선** → `token`/`dex_token`/`quote_token`. 없으면 `""` |
| `decimals` | int | 소수 자릿수 (기본 18) |
| `image_uri` | string | 아이콘 URL — **whitelist 우선** → registry. 없으면 `""` |

> 토큰 메타는 큐레이션된 `whitelist_token`(`enabled`)을 **최우선**으로 사용합니다. 화이트리스트에 있는 페어(예: USDT/WMON)는 registry의 빈/플레이스홀더 값 대신 큐레이션된 심볼·아이콘으로 렌더됩니다.

#### `FeeConfigInfo` (bps, `fee_config` 행 없으면 `null`)

| 필드 | 타입 | 설명 |
|------|------|------|
| `creator_bps` | int | 크리에이터 수수료 (bps, 1bps = 0.01%) |
| `curve_protocol_bps` | int | 커브 프로토콜 수수료 (bps) |
| `dex_protocol_bps` | int | DEX 프로토콜 수수료 (bps) |

---

## 6. `GET /dex/reserves?pool_id=`

온체인 `lpPair.getReserves()` 의 **백엔드 드롭인**. 리저브는 인덱서가 유지하는 `pool` 행에서 오므로 **RPC 호출이 없습니다**. 클라이언트가 `token0`/`token1` 기준으로 in/out 방향을 잡고 스왑 수식을 적용. 풀이 없으면 **404**.

| 항목 | 값 |
|------|----|
| Query | `pool_id` (필수) — 풀(페어) 주소(EIP-55 체크섬) |
| 200 | `ReservesResponse` |
| 400 | 잘못된 pool id / 404 풀 없음 |

### 응답 형태

```jsonc
{
  "pool_id":      "0x…",   // EIP-55
  "token0":       "0x…",   // EIP-55
  "token1":       "0x…",   // EIP-55
  "reserve0":     "15421418424704243305675",  // raw wei (token0 기준)
  "reserve1":     "335215354",                // raw wei (token1 기준)
  "block_number": 12345678  // 이 리저브 스냅샷의 인덱서 블록 — 체인 head 대비 신선도 판단용
}
```

> `reserve0`/`reserve1` 은 `token0`/`token1` 순서와 **일치**(페어 컨트랙트와 동일 정렬). 둘 다 raw wei이므로 표시·계산 시 각 토큰 `decimals`로 스케일하세요.

---

## 7. 공통 타입: `PoolInfo`

`/dex/pools` 응답과 `/dex/positions` 의 각 항목이 공유하는 풀 단위 정보 (지갑 무관).

| 필드 | 타입 | 설명 |
|------|------|------|
| `pool_id` | string | 페어 주소 (EIP-55) |
| `pair_label` | string | 사람이 읽는 페어 라벨, 예 `"WMON-USDT"` = `token0.symbol`+`"-"`+`token1.symbol` |
| `reserve0` | string | `pool.reserve0` (raw wei) |
| `reserve1` | string | `pool.reserve1` (raw wei) |
| `tvl_usd` | string | 풀 TVL USD 스냅샷 (`pool.value`, 인덱서 유지) |
| `total_supply` | string | 총 LP 공급 (raw wei, `pool.total_supply`) |
| `apr` | string \| null | 24h/7d/30d 윈도 중 **최대 LP-net APR**, 퍼센트 문자열 4자리 (예 `"130.0000"`). 데이터 없으면 `null` |

---

## 8. FE 체크리스트

- [ ] `/dex/search` 호출 전부 제거 → `/dex/tokens?q=` 로 교체
- [ ] 검색 입력을 **prefix** 가정으로 조정 (substring 기대 UX 제거)
- [ ] 컨트랙트 주소 붙여넣기 검색은 **full 40-hex 주소** 그대로 전달
- [ ] 모든 주소(`account`/`pool_id`/path)는 **EIP-55 체크섬** 으로 전달 (소문자 주소 금지)
- [ ] 보유 판정은 `balance != null` 로, 표시는 `balance / 10^decimals`
- [ ] external 배지/경고는 `token_type === "external"` 로 분기
- [ ] 세션 쿠키 의존 제거 — `/dex/*` 는 전부 무인증
- [ ] 스왑 견적은 `/dex/reserves` 의 `reserve0/1` + `decimals` 로 계산, `block_number` 로 신선도 확인
- [ ] 풀/포지션 토큰 아이콘·심볼은 응답값 그대로 사용 (whitelist 우선 소싱이 서버에서 적용됨)
