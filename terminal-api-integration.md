# 🌐 Terminal API Documentation (V1 + V2)

## 💡 Overview

The Terminal API provides GeckoTerminal-compatible blockchain data endpoints for the NAD.fun platform (token information, trading pairs, on-chain events).

**A single set of endpoints serves both V1 and V2 tokens** — there is no separate V1 vs V2 endpoint. The token's version and venue are distinguished by `dexKey` and `feeBps` (see the [DEX Key Values](#-dex-key-values) and [feeBps](#feebps) tables); the response **schema is identical across versions**. Routing, asset ordering, and pricing are derived from each token's **quote asset** (`quote_id`):

- **V1** tokens are WMON-quoted (`dexKey` `nadfun` on the bonding curve, `capricorn` after graduating to the Uniswap-v3 DEX; `feeBps` `100`).
- **V2** tokens add multi-quote (e.g. LVMON) and graduate to the internal NadSwap DEX (`dexKey` `nadfun-v2` on the curve, `nadswap` after graduation; `feeBps` from on-chain fee config). V2 broadened routing from "always WMON" to per-token `quote_id`, which is why ordering/pricing reference the quote asset rather than WMON.

This document supersedes the prior V1-only adapter doc — integrate these endpoints once to receive both V1 and V2 tokens.

**Scope:** nad.fun lifecycle tokens (a token has exactly one quote asset; token and quote are both 18 decimals). External DEX-listed token-to-token pairs are out of scope.

## 🔗 Base URLs

| Environment | Base URL |
|---|---|
| Testnet | `https://dev-api.nad.fun` |
| Mainnet | `https://api.nad.fun` |

Replace `{BASE_URL}` with one of the above.

## 📑 Table of Contents

1. [Get Latest Block](#1️⃣-get-latest-block)
2. [Get Asset Information](#2️⃣-get-asset-information)
3. [Get Pair Information](#3️⃣-get-pair-information)
4. [Get Blockchain Events](#4️⃣-get-blockchain-events)
5. [Get Token Metadata](#5️⃣-get-token-metadata)

---

## 1️⃣ Get Latest Block

### 📌 Basic Information

| Item | Description |
|---|---|
| URL | `/latest-block` |
| Method | `GET` |
| Description | Returns the latest indexed block number and timestamp |

### 🔧 Parameters

None.

### ✅ Success Response (200)

```json
{
  "block": {
    "blockNumber": 1234567,
    "blockTimestamp": 1704067200
  }
}
```

### 📊 Response Fields

| Field | Type | Description |
|---|---|---|
| `block.blockNumber` | Integer (u64) | Latest indexed block number |
| `block.blockTimestamp` | Integer (u64) | Unix timestamp (seconds) |

### 💡 Example

```bash
curl "{BASE_URL}/latest-block"
```

---

## 2️⃣ Get Asset Information

### 📌 Basic Information

| Item | Description |
|---|---|
| URL | `/asset` |
| Method | `GET` |
| Description | Retrieves asset (token) information by token address |

### 🔧 Parameters

| Parameter | Location | Type | Required | Description |
|---|---|---|---|---|
| `id` | Query | String | ✅ | Token contract address (42 chars, `0x` prefix) |

### ✅ Success Response (200)

```json
{
  "asset": {
    "id": "0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c",
    "name": "NadEarth",
    "symbol": "NAT",
    "decimals": 18,
    "totalSupply": "1000000000",
    "circulatingSupply": "547823.123456789012345678",
    "coinGeckoId": "monad"
  }
}
```

### 📊 Response Fields

| Field | Type | Description |
|---|---|---|
| `asset.id` | String | Token contract address |
| `asset.name` | String | Token name |
| `asset.symbol` | String | Token symbol |
| `asset.decimals` | Integer (u8) | Always `18` |
| `asset.totalSupply` | String (optional) | Always `"1000000000"` (1 billion) |
| `asset.circulatingSupply` | String (optional) | Decimalized (divided by 10^18) |
| `asset.coinGeckoId` | String (optional) | Always `"monad"` |

### ❌ Error Response (404)

```json
{ "error": "Asset not found" }
```

### 💡 Example

```bash
curl "{BASE_URL}/asset?id=0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c"
```

---

## 3️⃣ Get Pair Information

### 📌 Basic Information

| Item | Description |
|---|---|
| URL | `/pair` |
| Method | `GET` |
| Description | Retrieves the token's current trading pair |

### 🔧 Parameters

| Parameter | Location | Type | Required | Description |
|---|---|---|---|---|
| `id` | Query | String | ✅ | **Token** contract address (42 chars, `0x` prefix) |

> `id` is the token address, not a pair address. The endpoint returns the token's **current** pair (see [Known Limitations](#-known-limitations)).

### ✅ Success Response (200)

```json
{
  "pair": {
    "id": "0xD5724171C2b7f0AA717a324626050BD05767e2C6",
    "dexKey": "nadswap",
    "asset0Id": "0xBe3fa50514D9617ce645a02B34F595541AF02b6b",
    "asset1Id": "0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c",
    "createdAtBlockNumber": 1234567,
    "createdAtBlockTimestamp": 1704067200,
    "createdAtTxnId": "0x91a01448267c6959171ec75d9aac3c011f39346d7f3152b7f9f087519852d1d6",
    "creator": "0x9b834355d9EbcDFb291eAba6809B9E8D2C6b88d1",
    "feeBps": 555
  }
}
```

### 📊 Response Fields

| Field | Type | Description |
|---|---|---|
| `pair.id` | String | Pair id — pool address (DEX) or bonding-curve address (curve). See [pairId routing](#pairid-by-market-type) |
| `pair.dexKey` | String | DEX identifier. See [DEX Key Values](#-dex-key-values) |
| `pair.asset0Id` | String | First asset (ascending lowercase address) |
| `pair.asset1Id` | String | Second asset |
| `pair.createdAtBlockNumber` | Integer (u64, optional) | Pair creation block |
| `pair.createdAtBlockTimestamp` | Integer (u64, optional) | Pair creation timestamp |
| `pair.createdAtTxnId` | String (optional) | Pair creation tx hash |
| `pair.creator` | String (optional) | Creator wallet address |
| `pair.feeBps` | Integer (u32, optional) | Trading fee in basis points. See [feeBps](#feebps). V1 markets always return `100`; **omitted** only for **V2** markets when the token has no fee config |

> **Asset ordering:** `asset0`/`asset1` are ordered by ascending lowercase address, comparing the token against its **quote asset** (`quote_id`, e.g. WMON or LVMON) — not always WMON.

#### pairId by market type

| Market | `pair.id` |
|---|---|
| `CURVE` (V1) | V1 bonding-curve address |
| `V2_CURVE` | V2 bonding-curve address |
| `DEX` (V1) / `V2_DEX` | Pool address (falls back to the version's bonding-curve address if the pool id is unknown — should not happen for a real DEX market) |

#### feeBps

| Market | `feeBps` |
|---|---|
| `CURVE` / `DEX` (V1) | `100` (1%) |
| `V2_CURVE` | `creator_fee_rate + curve_protocol_fee_rate` |
| `V2_DEX` | `25 + creator_fee_rate + dex_protocol_fee_rate` (`25` = NadSwap LP fee, 0.25%) |

V1 (`CURVE` / `DEX`) always returns `100` regardless of fee config. For V2 markets (`V2_CURVE` / `V2_DEX`), `feeBps` is omitted entirely when the token has no fee config row.

### ❌ Error Response (404)

```json
{ "error": "Pair not found" }
```

### 💡 Example

```bash
curl "{BASE_URL}/pair?id=0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c"
```

---

## 4️⃣ Get Blockchain Events

### 📌 Basic Information

| Item | Description |
|---|---|
| URL | `/events` |
| Method | `GET` |
| Description | Retrieves `swap`, `join` (add liquidity), and `exit` (remove liquidity) events for a block range |

### 🔧 Parameters

| Parameter | Location | Type | Required | Description |
|---|---|---|---|---|
| `fromBlock` | Query | Integer (u64) | ✅ | Start block (inclusive) |
| `toBlock` | Query | Integer (u64) | ✅ | End block (inclusive). Max range `10000` |

### ✅ Success Response (200)

```json
{
  "events": [
    {
      "eventType": "swap",
      "block": { "blockNumber": 1234567, "blockTimestamp": 1704067200 },
      "txnId": "0x91a0...d1d6",
      "txnIndex": 5,
      "eventIndex": 12,
      "maker": "0x9b834355d9EbcDFb291eAba6809B9E8D2C6b88d1",
      "pairId": "0xD5724171C2b7f0AA717a324626050BD05767e2C6",
      "asset0In": "1000.5",
      "asset1Out": "0.000028",
      "priceNative": "0.000000028",
      "reserves": { "asset0": "12345.67", "asset1": "987654321.0" }
    },
    {
      "eventType": "join",
      "block": { "blockNumber": 1234568, "blockTimestamp": 1704067205 },
      "txnId": "0x82b0...e2e7",
      "txnIndex": 3,
      "eventIndex": 8,
      "maker": "0x8c72...9e01",
      "pairId": "0xD5724171C2b7f0AA717a324626050BD05767e2C6",
      "amount0": "100.0",
      "amount1": "5000.0",
      "reserves": { "asset0": "12445.67", "asset1": "992654321.0" }
    },
    {
      "eventType": "exit",
      "block": { "blockNumber": 1234569, "blockTimestamp": 1704067210 },
      "txnId": "0x93c0...f3f8",
      "txnIndex": 7,
      "eventIndex": 15,
      "maker": "0x7b61...8d90",
      "pairId": "0xD5724171C2b7f0AA717a324626050BD05767e2C6",
      "amount0": "50.0",
      "amount1": "2500.0",
      "reserves": { "asset0": "12395.67", "asset1": "990154321.0" }
    }
  ]
}
```

### 📊 Event Types

#### 🔄 Swap

| Field | Type | Description |
|---|---|---|
| `eventType` | String | Always `"swap"` |
| `block.blockNumber` / `block.blockTimestamp` | Integer (u64) | Block number / timestamp |
| `txnId` | String | Transaction hash |
| `txnIndex` | Integer (u32) | Transaction index in block |
| `eventIndex` | Integer (u32) | Log index within transaction |
| `maker` | String | Account that executed the swap |
| `pairId` | String | Pair the swap traded on — **per event** (see note) |
| `asset0In` / `asset1In` / `asset0Out` / `asset1Out` | String (optional) | Decimalized amounts in/out |
| `priceNative` | String | Price of `asset0` in `asset1` terms = `amount(asset1) / amount(asset0)` — this swap's execution price (same orientation as `reserves.asset1 / reserves.asset0`, but may differ slightly from the post-swap reserve ratio due to slippage). Pair-relative — GeckoTerminal derives each token's price from this + `asset0Id`/`asset1Id`, not a fixed token-in-quote price |
| `reserves.asset0` / `reserves.asset1` | String | Reserves after the swap (decimalized) |

> **Per-event `pairId`:** the pair is resolved from each event's own market type. A graduated token's curve-era swaps reference the bonding curve; its DEX-era swaps reference the pool.

> **Buy/Sell:** Buy = quote in → token out; Sell = token in → quote out. The exact `asset0/asset1` slot depends on which side `quote` sorts to (see [Asset Ordering](#-asset-ordering)).

#### ➕ Join (Add Liquidity) / ➖ Exit (Remove Liquidity)

| Field | Type | Description |
|---|---|---|
| `eventType` | String | `"join"` or `"exit"` |
| `block`, `txnId`, `txnIndex`, `eventIndex`, `maker` | — | As above |
| `pairId` | String | Pool address |
| `amount0` / `amount1` | String | Decimalized amounts added/removed |
| `reserves.asset0` / `reserves.asset1` | String | Reserves after the event (decimalized) |

### 📐 Event Ordering

Sorted ascending by: block number → transaction index → log/event index.

### 🔢 Number Formatting

- Decimalized by dividing by 10^18 (token and quote are both 18 decimals).
- Truncated to max 50 decimal places, trailing zeros removed.
- Plain decimal strings (no scientific notation).

### 💡 Example

```bash
LATEST=$(curl -s "{BASE_URL}/latest-block" | jq -r '.block.blockNumber')
curl "{BASE_URL}/events?fromBlock=$((LATEST-100))&toBlock=$LATEST"
```

---

## 5️⃣ Get Token Metadata

### 📌 Basic Information

| Item | Description |
|---|---|
| URL | `/{token_address}` |
| Method | `GET` |
| Description | Token image, description, and social links |

### 🔧 Parameters

| Parameter | Location | Type | Required | Description |
|---|---|---|---|---|
| `token_address` | Path | String | ✅ | Token contract address |

### ✅ Success Response (200)

```json
{
  "image": "https://storage.nadapp.net/coin/4e80d518-0533-4925-a8ef-cdf2c5ca56b8",
  "description": "Around the world!",
  "website": "https://testnet.nad.fun/v3/tokens/0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c",
  "twitter": "https://x.com/nadearth",
  "telegram": "https://t.me/nadearth"
}
```

### 📊 Response Fields

| Field | Type | Description |
|---|---|---|
| `image` | String | Token image URL |
| `description` | String | Token description |
| `website` | String | Defaults to the NAD.fun token page (`.../v3/tokens/{token}`) when unset |
| `twitter` | String (nullable) | X/Twitter URL |
| `telegram` | String (nullable) | Telegram URL |

### ❌ Error Response (404)

```json
{ "error": "Token not found" }
```

### 🚀 Caching

This response is cached in Redis.

### 💡 Example

```bash
curl "{BASE_URL}/0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c"
```

---

## ⚠️ Common Error Responses

```json
{ "error": "Invalid parameter: [details]" }   // 400
{ "error": "Resource not found" }              // 404
{ "error": "Internal server error" }           // 500
```

## 🚦 Rate Limiting

The market-data endpoints — `/latest-block`, `/asset`, `/pair`, `/events` — are exempt from rate limiting. The metadata endpoint `/{token_address}` is **not** exempt: like other non-exempt routes it is limited to 10 req/min without an API key, or 100 req/min with one (`X-API-Key`).

## ⏱️ Timeout Configuration

| Request Type | Default Timeout |
|---|---|
| GET requests | 10 seconds |
| Database queries | 10 seconds |

---

## 📚 Technical Notes

### 🔤 Asset Ordering

In `pair` and `swap`/`join`/`exit` responses, the two assets are ordered alphabetically by ascending lowercase address:

- `asset0` = the smaller address; `asset1` = the larger.
- The token is compared against its **quote asset** (`quote_id`). A token's quote is WMON or LVMON, not always WMON.

### 🔑 DEX Key Values

| `dexKey` | Market | Description |
|---|---|---|
| `nadfun` | `CURVE` | V1 bonding curve |
| `capricorn` | `DEX` | V1 graduated DEX (Uniswap v3) |
| `nadfun-v2` | `V2_CURVE` | V2 bonding curve |
| `nadswap` | `V2_DEX` | V2 graduated DEX (NadSwap / NadFunPair) |

### 📊 Constants

| Constant | Value | Description |
|---|---|---|
| Token Decimals | 18 | Always 18 |
| Quote Decimals | 18 | WMON / LVMON are 18 |
| Total Supply | 1,000,000,000 | Always 1 billion |
| NadSwap LP fee | 25 bps | 0.25%, included in V2_DEX `feeBps` |

### ⚠️ Known Limitations

- **Graduation seed liquidity** (the initial curve→pool mint at graduation) is recorded only at the pool level and does **not** appear as a `/events` `join`. Only user-initiated LP add/remove appears as `join`/`exit`.
- **`/pair` returns the token's current pair only.** A graduated token's past curve pair is reachable only via `/events` (each event carries its own `pairId`).

---

## 📋 Changelog

### V2 — quote-aware routing

- `/pair` and `/events` now route, order, and price by the token's `quote_id` (WMON or LVMON) instead of hardcoded WMON.
- `dexKey` adds `nadfun-v2` (V2 bonding curve) and `nadswap` (V2 DEX). V1 values (`nadfun`, `capricorn`) unchanged.
- `feeBps` is now derived from on-chain fee config for V2 markets (`V2_CURVE` = creator + curve protocol; `V2_DEX` = 25 + creator + dex protocol); omitted when no fee config exists. V1 stays fixed at `100`.
- Swap `pairId` is resolved per event from the event's market type (fixes curve-era swaps of graduated tokens previously pointing at the pool).
- **Response schemas are unchanged** — only values/behavior changed. Existing strict parsers continue to work.
