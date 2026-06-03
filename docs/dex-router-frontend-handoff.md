# Dex 라우터 변경 — 프론트엔드 핸드오프

> 대상: nadfun.com Frontend
> 브랜치: `feat/cms-whitelist-token` (base `v2`)
> 요약: **`GET /dex/search` 제거** → 검색은 `GET /dex/tokens?q=` 로 통합. Select Token 모달은 단일 엔드포인트로 동작.

---

## TL;DR

| 변경 | 내용 |
|------|------|
| ❌ **제거** | `GET /dex/search` — 호출 시 이제 **404** |
| ✅ **대체** | `GET /dex/tokens?q=<검색어>` — 리스팅 + 검색 통합 |
| 🔁 **무변경** | `GET /dex/positions/{account_id}`, `GET /dex/pools/{pool_id}` |

> **반드시 할 일:** 기존 `/dex/search` 호출 코드를 전부 `/dex/tokens?q=` 로 교체. 빠뜨리면 검색이 404로 깨집니다.

---

## 1. 현재 dex 라우터 (전체 엔드포인트)

| Method | Path | 인증 | 설명 | 상태 |
|--------|------|------|------|------|
| GET | `/dex/tokens` | 무인증 (`?account=` 옵션) | Select Token 리스트 + 검색(`?q=`) | 활성 |
| GET | `/dex/positions/{account_id}` | 무인증 | LP 포지션 조회 | 변경 없음 |
| GET | `/dex/pools/{pool_id}` | 무인증 | 풀 상세 | 변경 없음 |
| ~~GET~~ | ~~`/dex/search`~~ | ~~세션 필수~~ | ~~토큰 검색~~ | **제거됨** |

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
| `0x` + **정확히 40 hex** | **full CA** | 모든 테이블에서 해당 주소 1건 정확 조회 (**external 포함**) |
| `0x` + 40 미만 hex | **partial CA** | `token_id` prefix 매칭 (external 제외) |
| 그 외 텍스트 | **text** | `symbol`/`name` prefix(`q%`) 매칭 (external 제외) |

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
| `token_type` | string | `"whitelist"` \| `"nadfun_v2"` \| `"external"`. **렌더 분기·external 판정용** |
| `balance` | string \| null | **raw wei** 잔고(보유 시). 미보유 또는 `account` 미제공 시 `null` → **`balance != null` 로 보유 판정** |
| `balance_usd` | string \| null | 보유분 USD 가치. 미보유/미제공 시 `null` |

> `balance`는 **raw 정수 문자열(×10^decimals)** 입니다. 표시할 땐 FE에서 `decimals`로 나누세요. (`balance` / `10^decimals`)

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
      "balance": "1500000000000000000", "balance_usd": "4.32" },
    { "token_id": "0x…cC02", "symbol": "LVMON", "name": "…", "decimals": 18,
      "image_uri": "", "token_type": "whitelist", "balance": null, "balance_usd": null }
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

## 4. 변경 없는 엔드포인트 (참고)

| Method | Path | 설명 |
|--------|------|------|
| GET | `/dex/positions/{account_id}` | 계정 LP 포지션 (응답 스키마 동일) |
| GET | `/dex/pools/{pool_id}` | 풀 상세 (응답 스키마 동일) |

---

## 5. FE 체크리스트

- [ ] `/dex/search` 호출 전부 제거 → `/dex/tokens?q=` 로 교체
- [ ] 검색 입력을 **prefix** 가정으로 조정 (substring 기대 UX 제거)
- [ ] 컨트랙트 주소 붙여넣기 검색은 **full 40-hex 주소** 그대로 전달
- [ ] `account`는 **EIP-55 체크섬 주소** 로 전달 (소문자 주소 금지)
- [ ] 보유 판정은 `balance != null` 로, 표시는 `balance / 10^decimals`
- [ ] external 배지/경고는 `token_type === "external"` 로 분기
- [ ] 세션 쿠키 의존 제거 — `/dex/tokens` 는 무인증
