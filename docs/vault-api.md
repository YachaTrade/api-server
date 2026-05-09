# Vault API 문서

## 개요

Vault API는 V2 토큰이 거래 수수료를 어떤 vault에 어떤 비율로 분배하는지,
그리고 각 vault의 누적 활동 통계를 조회하는 API입니다.

- **Token Vault 목록 조회**: 토큰이 fee를 라우팅하는 vault 목록 + 비율(bps) + vault별 stats

핵심 데이터 소스 (V2 vault schema):

| 테이블 | 역할 |
|---|---|
| `v2_creator_fee_allocation` | 토큰 ↔ vault 멤버십 + bps |
| `v2_vault_metadata` | vault 카탈로그 (vault_type, name, active) |
| `v2_burn_vault_stats` | Buyback & Burn 통계 |
| `v2_lp_vault_stats` | LP Support 통계 |
| `v2_creator_fee_vault_stats` | Creator 분배 통계 |
| `v2_gift_vault_stats` | Gift vault 통계 (lifecycle, receiver) |
| `market` + `quote_token` | LP `pool_pair` 라벨 합성 (token symbol + quote symbol) |

---

## API 엔드포인트

### 1. Token Vault 목록 조회 (`GET /vault/{token_id}`)

특정 토큰이 거래 수수료를 라우팅하는 모든 vault의 목록과 비율, 누적 통계를 반환합니다.

#### 요청
- **Method**: `GET`
- **인증**: 불필요
- **캐시**: 30초 (env `GET_TOKEN_VAULTS_RESPONSE_EXPIRATION` 으로 조정 가능, 단위: ms)

#### Path Parameters

| 파라미터 | 타입 | 필수 | 설명 |
|----------|------|------|------|
| `token_id` | string | O | 토큰 컨트랙트 주소 (EVM 형식, EIP-55 checksum) |

#### 응답 구조

응답은 vault별로 **공통 필드** + `vault_type` 기반 **tagged stats**로 구성됩니다.

```jsonc
{
  "token_id": "0x350035555E10d9AfAF1566AaebfCeD5BA6C27777",
  "quote_id": "0x5a4E0bFDeF88C9032CB4d24338C5EB3d3870BfDd",      // distributed_quote 단위
  "total_distributed_quote": "4125821524927906943762",            // 모든 vault distributed_quote 합
  "vaults": [
    {
      "vault_id": "0x...",            // vault 컨트랙트 주소
      "bps": 4500,                    // 0~10000 (45.00%)
      "name": "Buyback & Burn",       // v2_vault_metadata.name
      "active": true,                 // v2_vault_metadata.active
      "distributed_quote": "1444037533724767430315", // 이 vault에 누적 분배된 fee (quote 단위)
      "last_executed_at": 1714560000, // 해당 vault stat 테이블의 updated_at
      "vault_type": "BURN",           // 디스크리미네이터 (아래 5종)
      "stats": { /* vault_type 별 고유 필드 */ }
    }
  ]
}
```

`distributed_quote` / `total_distributed_quote`는 모두 `quote_id`로 명시된 quote 토큰 단위(wei)이며, 출처는 `v2_creator_fee_distribution_stats` (트리거가 `v2_creator_fee_distribution.event_type='DISTRIBUTE'` 행을 누적). 토큰이 vault에 분배한 적이 없으면 `"0"`.

`vaults` 배열은 `bps` 내림차순 정렬됩니다.

#### vault_type 별 stats

##### `"BURN"` (Buyback & Burn)

```json
{
  "vault_type": "BURN",
  "stats": {
    "quote_spent": "100000000000000000000000",
    "tokens_burned": "12345678900000000000000",
    "execution_count": 3
  }
}
```

| 필드 | 타입 | 설명 |
|---|---|---|
| `quote_spent` | string | 토큰 매수에 사용한 누적 MON (wei) |
| `tokens_burned` | string | 영구 소각된 누적 토큰 (wei) |
| `execution_count` | number | 누적 buyback+burn 실행 횟수 |

##### `"LP"` (LP Support)

```json
{
  "vault_type": "LP",
  "stats": {
    "quote_injected": "100000000000000000000000",
    "token_injected": "...",
    "lp_burned": "...",
    "pool_pair": "ATOM/WMON",
    "execution_count": 2
  }
}
```

| 필드 | 타입 | 설명 |
|---|---|---|
| `quote_injected` | string | LP에 주입한 누적 quote 자산 (wei) |
| `token_injected` | string | LP에 주입한 누적 토큰 (wei) |
| `lp_burned` | string | 잠금/소각된 누적 LP 토큰 (wei) |
| `pool_pair` | string | `"{token.symbol}/{quote_token.symbol}"` 라벨 (UI 표시용) |
| `execution_count` | number | 누적 LP injection 횟수 |

##### `"CREATOR_FEE"` (Creator)

```json
{
  "vault_type": "CREATOR_FEE",
  "stats": {
    "current_balance": "0",
    "total_deposited": "100000000000000000000000",
    "total_claimed": "100000000000000000000000",
    "deposit_count": 5,
    "claim_count": 1
  }
}
```

| 필드 | 타입 | 설명 |
|---|---|---|
| `current_balance` | string | 미수령 잔액 (wei). claim 후 0 |
| `total_deposited` | string | vault에 들어온 누적 MON |
| `total_claimed` | string | 크리에이터가 가져간 누적 MON |
| `deposit_count` | number | DEPOSIT 이벤트 누적 횟수 |
| `claim_count` | number | CLAIM 이벤트 누적 횟수 |

##### `"GIFT"` (Gift Vault)

```json
{
  "vault_type": "GIFT",
  "stats": {
    "current_state": "Active",
    "current_balance": "0",
    "total_deposited": "100000000000000000000000",
    "total_claimed": "100000000000000000000000",
    "total_expired": "0",
    "platform": "X",
    "platform_id": "@Beakdoong",
    "receiver": "0x...",
    "buyback_quote_spent": "0",
    "buyback_tokens": "0"
  }
}
```

| 필드 | 타입 | 설명 |
|---|---|---|
| `current_state` | string | `"Accumulating"` \| `"Active"` \| `"Burned"` |
| `current_balance` | string | 미수령 잔액 (wei) |
| `total_deposited` | string | vault에 들어온 누적 MON |
| `total_claimed` | string | receiver가 받은 누적 MON |
| `total_expired` | string | 만료 sweep으로 소각된 누적 MON |
| `platform` | string \| null | `"X"` \| `"GITHUB"` (SETUP 시 결정) |
| `platform_id` | string \| null | 플랫폼별 핸들 (예: `"@Beakdoong"`) |
| `receiver` | string \| null | 온체인 wallet 주소 (`RECEIVER_SET` 후 채워짐) |
| `buyback_quote_spent` | string | 만료 시 buyback에 쓴 MON |
| `buyback_tokens` | string | 만료 시 소각한 토큰 |

##### `"CUSTOM"`

표준 4종(BURN/LP/CREATOR_FEE/GIFT) 외 vault. `stats`는 빈 객체 `{}`.
프론트엔드는 `vault_id` / `bps` / `name` 만으로 fallback 표시 가능.

```json
{
  "vault_type": "CUSTOM",
  "stats": {}
}
```

#### 에러 응답
- `400`: 잘못된 token address (EIP-55 형식 아님)
- `500`: 내부 서버 에러 (DB 조회 실패 등)

#### 빈 응답

토큰에 등록된 vault allocation이 없으면 `vaults: []` 빈 배열을 반환합니다.
404가 아닙니다.

```json
{ "token_id": "0x...", "quote_id": "0x...", "total_distributed_quote": "0", "vaults": [] }
```

---

## TypeScript 인터페이스

```typescript
type VaultType = "BURN" | "LP" | "CREATOR_FEE" | "GIFT" | "CUSTOM";

interface VaultEntryBase {
  vault_id: string;
  bps: number;                  // 0..10000
  name: string;
  active: boolean;
  distributed_quote: string;    // 이 vault에 누적 분배된 fee (quote 단위, wei)
  last_executed_at: number;
}

type VaultEntry = VaultEntryBase & (
  | { vault_type: "BURN";        stats: BurnStats }
  | { vault_type: "LP";          stats: LpStats }
  | { vault_type: "CREATOR_FEE"; stats: CreatorFeeStats }
  | { vault_type: "GIFT";        stats: GiftStats }
  | { vault_type: "CUSTOM";      stats: Record<string, never> }
);

interface BurnStats {
  quote_spent: string;
  tokens_burned: string;
  execution_count: number;
}

interface LpStats {
  quote_injected: string;
  token_injected: string;
  lp_burned: string;
  pool_pair: string;
  execution_count: number;
}

interface CreatorFeeStats {
  current_balance: string;
  total_deposited: string;
  total_claimed: string;
  deposit_count: number;
  claim_count: number;
}

interface GiftStats {
  current_state: "Accumulating" | "Active" | "Burned";
  current_balance: string;
  total_deposited: string;
  total_claimed: string;
  total_expired: string;
  platform: "X" | "GITHUB" | null;
  platform_id: string | null;
  receiver: string | null;
  buyback_quote_spent: string;
  buyback_tokens: string;
}

interface TokenVaultsResponse {
  token_id: string;
  quote_id: string;                 // distributed_quote 단위 (market.quote_id)
  total_distributed_quote: string;  // SUM of vaults[].distributed_quote
  vaults: VaultEntry[];
}
```

---

## UI 매핑 (참고)

`Fee Allocation` 도넛 차트와 `Fee Sharing Strategy` 패널에 그대로 매핑됩니다:

| UI 영역 | 응답 필드 |
|---|---|
| 파이 차트 % | `vaults[].bps` (10000으로 나눠서 %) |
| 색상별 라벨 | `vaults[].name` |
| Buyback & Burn → Total Burned | `BurnStats.quote_spent` (또는 `tokens_burned`) |
| Buyback & Burn → Executed N Times | `BurnStats.execution_count` |
| Gift → Total Sent | `GiftStats.total_claimed` |
| Gift → Owner's X Handle | `GiftStats.platform_id` (단, `platform === "X"`) |
| Gift → Wallet Address | `GiftStats.receiver` |
| LP Support → Total Added | `LpStats.quote_injected` |
| LP Support → Pool | `LpStats.pool_pair` |
| Creator → Total Distributed | `CreatorFeeStats.total_deposited` (또는 `total_claimed`) |
| Last Executed (모든 섹션) | `vaults[].last_executed_at` (unix seconds) |
