# nad.fun API V2 — Interface Changes

This document lists every API response schema change between the current mainnet (V1) API and the upcoming V2 release. Only type/field changes are described — no migration guides, no infrastructure notes.

---

## Summary

| Type | Change |
|---|---|
| `TokenInfo` | `hackathon_info` removed, `version` added |
| `MarketInfo` | `quote_id`, `reserve_quote`, `quote_price`, `ath_price_quote` added |
| `SwapInfo` | `quote_amount`, `quote_price` added |
| `MarketType` enum | `V2_CURVE`, `V2_DEX` added |
| `TokenVersion` enum | new type (`V1` \| `V2`) |
| `HackathonInfo` + sub-types | removed entirely |
| Endpoints | `/token/hackathon`, `/order/hackathon`, `/cms/hackathon/register` removed |

---

## TokenInfo

### Before (V1)

```typescript
interface TokenInfo {
    token_id: string;
    name: string;
    symbol: string;
    image_uri: string;
    description: string | null;
    is_graduated: boolean;
    is_nsfw: boolean;
    twitter: string | null;
    telegram: string | null;
    website: string | null;
    created_at: number;
    creator: AccountInfo;
    is_cto: boolean;
    hackathon_info: HackathonInfo | null;   // ← REMOVED in V2
}
```

### After (V2)

```typescript
interface TokenInfo {
    token_id: string;
    name: string;
    symbol: string;
    image_uri: string;
    description: string | null;
    is_graduated: boolean;
    is_nsfw: boolean;
    twitter: string | null;
    telegram: string | null;
    website: string | null;
    created_at: number;
    creator: AccountInfo;
    is_cto: boolean;
    version: TokenVersion;                   // ← NEW: "V1" | "V2"
}
```

### Changes

- **Removed**: `hackathon_info` — field and all nested `HackathonInfo` sub-types no longer exist in any response
- **Added**: `version` — token version (`"V1"` or `"V2"`)

---

## MarketInfo

### Before (V1)

```typescript
interface MarketInfo {
    market_type: MarketType;
    token_id: string;
    market_id: string;
    reserve_native: string;
    reserve_token: string;
    token_price: string;
    native_price: string;
    price: string;
    price_usd: string;
    price_native: string;
    total_supply: string;
    volume: string;
    ath_price: string;
    ath_price_usd: string;
    ath_price_native: string;
    holder_count: number;
}
```

### After (V2)

```typescript
interface MarketInfo {
    market_type: MarketType;
    token_id: string;
    quote_id: string;             // ← NEW
    market_id: string;
    reserve_native: string;
    reserve_quote: string;        // ← NEW
    reserve_token: string;
    token_price: string;
    native_price: string;
    quote_price: string;          // ← NEW
    price: string;
    price_usd: string;
    price_native: string;
    total_supply: string;
    volume: string;
    ath_price: string;
    ath_price_usd: string;
    ath_price_native: string;
    ath_price_quote: string;      // ← NEW
    holder_count: number;
}
```

### Changes

- **Added**: `quote_id` — quote asset address. WMON for V1 tokens; may be another address (USDC, etc.) for V2 tokens.
- **Added**: `reserve_quote` — quote asset reserve. Identical to `reserve_native` for V1/WMON tokens.
- **Added**: `quote_price` — quote asset / USD price. Identical to `native_price` for V1/WMON tokens.
- **Added**: `ath_price_quote` — all-time-high price in quote asset. Identical to `ath_price_native` for V1/WMON tokens.

---

## SwapInfo

### Before (V1)

```typescript
interface SwapInfo {
    event_type: SwapType;
    native_amount: string;
    token_amount: string;
    native_price: string;
    value: string;
    transaction_hash: string;
    created_at: number;
}
```

### After (V2)

```typescript
interface SwapInfo {
    event_type: SwapType;
    native_amount: string;
    quote_amount: string;         // ← NEW
    token_amount: string;
    native_price: string;
    quote_price: string;          // ← NEW
    value: string;
    transaction_hash: string;
    created_at: number;
}
```

### Changes

- **Added**: `quote_amount` — quote asset amount swapped. Identical to `native_amount` for V1/WMON tokens.
- **Added**: `quote_price` — quote asset / USD price at swap time. Identical to `native_price` for V1/WMON tokens.

---

## MarketType (enum)

### Before (V1)

```typescript
type MarketType = "CURVE" | "DEX";
```

### After (V2)

```typescript
type MarketType = "CURVE" | "DEX" | "V2_CURVE" | "V2_DEX";
```

### Changes

- **Added**: `"V2_CURVE"` — V2 bonding curve market
- **Added**: `"V2_DEX"` — V2 graduated DEX market

Note: `V2_CURVE` / `V2_DEX` values will only appear in responses once V2 tokens exist in the database. Current mainnet data is all V1.

---

## TokenVersion (new enum)

### New in V2

```typescript
type TokenVersion = "V1" | "V2";
```

Used as the value of `TokenInfo.version`. All existing mainnet tokens have `version: "V1"`.

---

## HackathonInfo (removed)

### Before (V1)

`TokenInfo.hackathon_info` was an optional nested object containing team, project, and registration metadata for hackathon-registered tokens. The full type tree (`HackathonInfo`, `HackathonTeamInfo`, `HackathonProjectInfo`, etc.) is defined in `src/types/hackathon/` in V1.

### After (V2)

All `hackathon_*` types are removed. `TokenInfo.hackathon_info` no longer exists. Clients that read `token.hackathon_info` will see `undefined`.

---

## Removed endpoints

The following endpoints are removed in V2 and will return `404`:

| Method | Path | Purpose |
|---|---|---|
| GET | `/token/hackathon` | List hackathon tokens |
| GET | `/order/hackathon` | Ordered hackathon tokens |
| POST | `/cms/hackathon/register` | Register a token for hackathon |

Clients calling these endpoints must remove the call. No replacement is provided.

---

## Field name vs. semantics

The `native_*` field names (`native_price`, `native_amount`, `reserve_native`) are retained for backward compatibility. For V1/WMON tokens they carry the same meaning as before. For V2 non-WMON quote tokens (when they exist), these fields hold the quote-asset value rather than MON value — the `quote_*` fields are the canonical source going forward.

Since current mainnet data is all V1/WMON, this semantic drift is not observable today.
