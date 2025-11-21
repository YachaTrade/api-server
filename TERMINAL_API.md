# Terminal API Documentation

## Overview
The Terminal API provides Gecko Terminal-compatible blockchain data endpoints for the NAD.fun platform. This API enables real-time access to token information, trading pairs, and blockchain events.

## Base URLs

| Environment | Base URL |
| --- | --- |
| Testnet | https://dev-api.nad.fun |
| Mainnet | https://api.nad.fun |

---

## Table of Contents
1. [Get Latest Block](#1-get-latest-block)
2. [Get Asset Information](#2-get-asset-information)
3. [Get Pair Information](#3-get-pair-information)
4. [Get Blockchain Events](#4-get-blockchain-events)
5. [Get Token Metadata](#5-get-token-metadata)

---

# 1. Get Latest Block

### Basic Information

| Item | Description |
| --- | --- |
| URL | /latest-block |
| Method | GET |
| Description | Returns the latest indexed block number and timestamp from the blockchain |

### Parameters

None

### Response

#### Success Response (Status: 200)

```json
{
  "block": {
    "blockNumber": 1234567,
    "blockTimestamp": 1704067200
  }
}
```

### Response Fields

| Field | Type | Description |
| --- | --- | --- |
| block.blockNumber | Integer (u64) | Latest indexed block number |
| block.blockTimestamp | Integer (u64) | Unix timestamp in seconds |

### Example Requests

Testnet:
```bash
curl https://dev-api.nad.fun/latest-block
```

Mainnet:
```bash
curl https://api.nad.fun/latest-block
```

---

# 2. Get Asset Information

### Basic Information

| Item | Description |
| --- | --- |
| URL | /asset |
| Method | GET |
| Description | Retrieves asset (token) information by token address |

### Parameters

| Parameter | Location | Type | Required | Description |
| --- | --- | --- | --- | --- |
| id | Query | String | Required | Token contract address (42 characters with 0x prefix) |

### Response

#### Success Response (Status: 200)

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

### Response Fields

| Field | Type | Description |
| --- | --- | --- |
| asset.id | String | Token contract address |
| asset.name | String | Token name |
| asset.symbol | String | Token symbol |
| asset.decimals | Integer (u8) | Token decimals (always 18) |
| asset.totalSupply | String (optional) | Total token supply (always "1000000000" = 1 billion) |
| asset.circulatingSupply | String (optional) | Circulating supply (decimalized, removed 18 decimals) |
| asset.coinGeckoId | String (optional) | CoinGecko ID (always "monad") |

#### Error Response (Status: 404)

```json
{
  "error": "Asset not found"
}
```

### Example Requests

Testnet:
```bash
curl "https://dev-api.nad.fun/asset?id=0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c"
```

Mainnet:
```bash
curl "https://api.nad.fun/asset?id=0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c"
```

---

# 3. Get Pair Information

### Basic Information

| Item | Description |
| --- | --- |
| URL | /pair |
| Method | GET |
| Description | Retrieves trading pair information for a token |

### Parameters

| Parameter | Location | Type | Required | Description |
| --- | --- | --- | --- | --- |
| id | Query | String | Required | Token contract address (42 characters with 0x prefix) |

### Response

#### Success Response (Status: 200)

```json
{
  "pair": {
    "id": "0xD5724171C2b7f0AA717a324626050BD05767e2C6",
    "dexKey": "nadfun",
    "asset0Id": "0x1234567890123456789012345678901234567890",
    "asset1Id": "0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c",
    "createdAtBlockNumber": 1234567,
    "createdAtBlockTimestamp": 1704067200,
    "createdAtTxnId": "0x91a01448267c6959171ec75d9aac3c011f39346d7f3152b7f9f087519852d1d6",
    "creator": "0x9b834355d9EbcDFb291eAba6809B9E8D2C6b88d1",
    "feeBps": 100
  }
}
```

### Response Fields

| Field | Type | Description |
| --- | --- | --- |
| pair.id | String | Pool ID or bonding curve contract address |
| pair.dexKey | String | DEX identifier: "nadfun" (bonding curve) or "capricorn" (DEX) |
| pair.asset0Id | String | First asset address (alphabetically ordered) |
| pair.asset1Id | String | Second asset address (alphabetically ordered) |
| pair.createdAtBlockNumber | Integer (u64, optional) | Block number when pair was created |
| pair.createdAtBlockTimestamp | Integer (u64, optional) | Unix timestamp of pair creation |
| pair.createdAtTxnId | String (optional) | Transaction hash of pair creation |
| pair.creator | String (optional) | Creator wallet address |
| pair.feeBps | Integer (u32, optional) | Trading fee in basis points (always 100 = 1%) |

**Note:** Assets are always ordered alphabetically by comparing token address with WMON address.

#### Error Response (Status: 404)

```json
{
  "error": "Pair not found"
}
```

### Example Requests

Testnet:
```bash
curl "https://dev-api.nad.fun/pair?id=0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c"
```

Mainnet:
```bash
curl "https://api.nad.fun/pair?id=0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c"
```

---

# 4. Get Blockchain Events

### Basic Information

| Item | Description |
| --- | --- |
| URL | /events |
| Method | GET |
| Description | Retrieves all blockchain events (swap, join, exit) for a specified block range |

### Parameters

| Parameter | Location | Type | Required | Description |
| --- | --- | --- | --- | --- |
| fromBlock | Query | Integer (u64) | Required | Starting block number (inclusive) |
| toBlock | Query | Integer (u64) | Required | Ending block number (inclusive) |

### Response

#### Success Response (Status: 200)

```json
{
  "events": [
    {
      "eventType": "swap",
      "block": {
        "blockNumber": 1234567,
        "blockTimestamp": 1704067200
      },
      "txnId": "0x91a01448267c6959171ec75d9aac3c011f39346d7f3152b7f9f087519852d1d6",
      "txnIndex": 5,
      "eventIndex": 12,
      "maker": "0x9b834355d9EbcDFb291eAba6809B9E8D2C6b88d1",
      "pairId": "0xD5724171C2b7f0AA717a324626050BD05767e2C6",
      "asset0In": "0",
      "asset1In": "1000.5",
      "asset0Out": "0.000028",
      "asset1Out": "0",
      "priceNative": "0.000000028",
      "reserves": {
        "asset0": "12345.67",
        "asset1": "987654321.0"
      }
    },
    {
      "eventType": "join",
      "block": {
        "blockNumber": 1234568,
        "blockTimestamp": 1704067205
      },
      "txnId": "0x82b01558378c7969172fc86e0bbd4d122g40457e8g4263c8g0g198630963e2e7",
      "txnIndex": 3,
      "eventIndex": 8,
      "maker": "0x8c723244e8DcCEe384dFba5909A9F9E7D1B99e01",
      "pairId": "0xD5724171C2b7f0AA717a324626050BD05767e2C6",
      "amount0": "100.0",
      "amount1": "5000.0",
      "reserves": {
        "asset0": "12445.67",
        "asset1": "992654321.0"
      }
    },
    {
      "eventType": "exit",
      "block": {
        "blockNumber": 1234569,
        "blockTimestamp": 1704067210
      },
      "txnId": "0x93c02669489d8a7a283gd97f1cce5e233h51568f9h5374d9h1h209741074f3f8",
      "txnIndex": 7,
      "eventIndex": 15,
      "maker": "0x7b612133d7DbBDdd273dCa4808A8E8D6C0A88d90",
      "pairId": "0xD5724171C2b7f0AA717a324626050BD05767e2C6",
      "amount0": "50.0",
      "amount1": "2500.0",
      "reserves": {
        "asset0": "12395.67",
        "asset1": "990154321.0"
      }
    }
  ]
}
```

### Event Types

#### Swap Event

| Field | Type | Description |
| --- | --- | --- |
| eventType | String | Always "swap" |
| block.blockNumber | Integer (u64) | Block number of the event |
| block.blockTimestamp | Integer (u64) | Unix timestamp of the block |
| txnId | String | Transaction hash |
| txnIndex | Integer (u32) | Transaction index in block |
| eventIndex | Integer (u32) | Log index within transaction |
| maker | String | Account address that executed the swap |
| pairId | String | Pool ID or bonding curve address |
| asset0In | String (optional) | Amount of asset0 sent in (decimalized, max 50 decimals) |
| asset1In | String (optional) | Amount of asset1 sent in (decimalized, max 50 decimals) |
| asset0Out | String (optional) | Amount of asset0 received (decimalized, max 50 decimals) |
| asset1Out | String (optional) | Amount of asset1 received (decimalized, max 50 decimals) |
| priceNative | String | Price of asset0 in asset1 terms |
| reserves.asset0 | String | Reserve amount of asset0 after swap (decimalized) |
| reserves.asset1 | String | Reserve amount of asset1 after swap (decimalized) |

**Swap Logic:**
- Buy: native (asset1) goes in → token (asset0) comes out
- Sell: token (asset0) goes in → native (asset1) comes out
- Price is calculated as: asset1_amount / asset0_amount

#### Join Event (Add Liquidity)

| Field | Type | Description |
| --- | --- | --- |
| eventType | String | Always "join" |
| block.blockNumber | Integer (u64) | Block number of the event |
| block.blockTimestamp | Integer (u64) | Unix timestamp of the block |
| txnId | String | Transaction hash |
| txnIndex | Integer (u32) | Transaction index in block |
| eventIndex | Integer (u32) | Log index within transaction |
| maker | String | Account address that added liquidity |
| pairId | String | Pool ID or bonding curve address |
| amount0 | String | Amount of asset0 added (decimalized, max 50 decimals) |
| amount1 | String | Amount of asset1 added (decimalized, max 50 decimals) |
| reserves.asset0 | String | Reserve amount of asset0 after addition (decimalized) |
| reserves.asset1 | String | Reserve amount of asset1 after addition (decimalized) |

#### Exit Event (Remove Liquidity)

| Field | Type | Description |
| --- | --- | --- |
| eventType | String | Always "exit" |
| block.blockNumber | Integer (u64) | Block number of the event |
| block.blockTimestamp | Integer (u64) | Unix timestamp of the block |
| txnId | String | Transaction hash |
| txnIndex | Integer (u32) | Transaction index in block |
| eventIndex | Integer (u32) | Log index within transaction |
| maker | String | Account address that removed liquidity |
| pairId | String | Pool ID or bonding curve address |
| amount0 | String | Amount of asset0 removed (decimalized, max 50 decimals) |
| amount1 | String | Amount of asset1 removed (decimalized, max 50 decimals) |
| reserves.asset0 | String | Reserve amount of asset0 after removal (decimalized) |
| reserves.asset1 | String | Reserve amount of asset1 after removal (decimalized) |

### Event Ordering

All events are sorted chronologically by:
1. Block number (ascending)
2. Transaction index (ascending)
3. Event index / log index (ascending)

### Number Formatting

All numeric amounts (asset amounts, reserves, prices):
- Decimalized by dividing by 10^18
- Truncated to maximum 50 decimal places
- Normalized to remove trailing zeros
- Returned as plain strings (no scientific notation)

### Example Requests

Testnet:
```bash
curl "https://dev-api.nad.fun/events?fromBlock=100&toBlock=200"
```

Mainnet:
```bash
curl "https://api.nad.fun/events?fromBlock=100&toBlock=200"
```

Query recent 100 blocks:
```bash
# Get latest block first
LATEST=$(curl -s https://api.nad.fun/latest-block | jq -r '.block.blockNumber')
FROM=$((LATEST - 100))

# Query events
curl "https://api.nad.fun/events?fromBlock=$FROM&toBlock=$LATEST"
```

---

# 5. Get Token Metadata

### Basic Information

| Item | Description |
| --- | --- |
| URL | /:token_address |
| Method | GET |
| Description | Retrieves terminal metadata for a specific token (image, description, social links) |

### Parameters

| Parameter | Location | Type | Required | Description |
| --- | --- | --- | --- | --- |
| token_address | Path | String | Required | Token contract address (42 characters with 0x prefix) |

### Response

#### Success Response (Status: 200)

```json
{
  "image": "https://storage.nadapp.net/coin/4e80d518-0533-4925-a8ef-cdf2c5ca56b8",
  "description": "Around the world!",
  "website": "https://testnet.nad.fun/v3/tokens/0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c",
  "twitter": "https://x.com/nadearth",
  "telegram": "https://t.me/nadearth"
}
```

### Response Fields

| Field | Type | Description |
| --- | --- | --- |
| image | String | Token image URL |
| description | String | Token description |
| website | String | Website URL (defaults to NAD.fun token page if not set) |
| twitter | String (nullable) | X/Twitter profile URL |
| telegram | String (nullable) | Telegram group URL |

**Note:** If website is not set in database, it defaults to:
- Testnet: `https://testnet.nad.fun/v3/tokens/{token_address}`
- Mainnet: `https://nad.fun/v3/tokens/{token_address}`

#### Error Response (Status: 404)

```json
{
  "error": "Token not found"
}
```

### Caching

This endpoint response is cached in Redis for performance optimization.

### Example Requests

Testnet:
```bash
curl https://dev-api.nad.fun/0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c
```

Mainnet:
```bash
curl https://api.nad.fun/0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c
```

---

## Common Error Responses

All endpoints may return the following error responses:

### Bad Request (Status: 400)

```json
{
  "error": "Invalid parameter: [details]"
}
```

### Not Found (Status: 404)

```json
{
  "error": "Resource not found"
}
```

### Internal Server Error (Status: 500)

```json
{
  "error": "Internal server error"
}
```

---

## Rate Limiting

No explicit rate limiting is currently enforced on Terminal API endpoints.

---

## Timeout Configuration

| Request Type | Default Timeout |
| --- | --- |
| GET requests | 10 seconds |
| POST requests | 15 seconds |
| Database queries | 10 seconds |

---

## Technical Notes

### Asset Ordering

In pair and event responses, assets are ordered alphabetically by comparing token addresses:
- `asset0`: The asset with the smaller address (alphabetically)
- `asset1`: The asset with the larger address (alphabetically)

WMON (Wrapped MON, native currency) is typically compared against the project token address.

### DEX Key Values

| dexKey | Description |
| --- | --- |
| nadfun | Bonding Curve market (CURVE type) |
| capricorn | Uniswap v3 DEX market (DEX type) |

### Constants

- **Token Decimals**: Always 18 for all tokens
- **Total Supply**: Always 1,000,000,000 (1 billion)
- **Trading Fee**: Always 100 basis points (1%)

---

## Changelog

### Latest Updates (2025-01-20)

- Renamed API from "Gecko Terminal" to "Terminal API"
- Added support for join (mint/add liquidity) events
- Added support for exit (burn/remove liquidity) events
- Fixed BigDecimal truncation to 50 decimal places for all numeric values
- Updated market info to include reserve_native and reserve_token fields
- Improved number formatting to prevent scientific notation

### Recent Commits

- `b829db0` - Renamed gecko to terminal throughout codebase
- `6b7b68c` - Fixed BigDecimal truncation issue
- `3728a62` - Added join and exit event support
- `85c0198` - Updated market info with reserve fields
