# API Documentation

## Overview

This document provides the complete API specification for the NADS Pump API Server.

---

## Table of Contents

1. [Get Token Metadata API](#1-get-token-metadata-api)
2. [Get Market Data API](#2-get-market-data-api)
3. [Get Chart Data API](#3-get-chart-data-api)
4. [Get Token Metrics API](#4-get-token-metrics-api)
5. [Upload Image API](#5-upload-image-api)
6. [Upload Metadata API](#6-upload-metadata-api)
7. [Mine Salt API](#7-mine-salt-api)

---

# 1. Get Token Metadata API

### Basic Information

| Item        | Description                                                       |
| ----------- | ----------------------------------------------------------------- |
| URL         | /token/metadata/:token_id                                         |
| Method      | GET                                                               |
| Description | Query comprehensive metadata and market data for a specific token |

### Parameters

| Parameter | Location | Type   | Required | Description                                               |
| --------- | -------- | ------ | -------- | --------------------------------------------------------- |
| token_id  | Path     | String | Required | EVM token address to query (42 characters with 0x prefix) |

### Response

#### Success Response (Status: 200)

```json
{
  "token_info": {
    "token_id": "0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c",
    "name": "NadEarth",
    "symbol": "NAT",
    "image_uri": "https://storage.nadapp.net/coin/4e80d518-0533-4925-a8ef-cdf2c5ca56b8",
    "description": "Around the world!",
    "is_graduated": false,
    "is_nsfw": false,
    "twitter": "https://twitter.com/nadearth",
    "telegram": "https://t.me/nadearth",
    "website": "https://nadearth.com",
    "created_at": 1754927984,
    "creator": {
      "account_id": "0x9b834355d9EbcDFb291eAba6809B9E8D2C6b88d1",
      "nickname": "Creator Name",
      "bio": "Token creator",
      "image_uri": "https://storage.nadapp.net/profile/..."
    }
  },
  "market_info": {
    "market_type": "CURVE",
    "token_id": "0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c",
    "market_id": "0xD5724171C2b7f0AA717a324626050BD05767e2C6",
    "reserve_native": "12345.67",
    "reserve_token": "987654321.0",
    "token_price": "0.00000028",
    "native_price": "1.25",
    "price": "2.8000E-8",
    "total_supply": "1000000000000000000000000000",
    "volume": "50000.0",
    "ath_price": "0.00000035",
    "holder_count": 142
  }
}
```

### Response Fields

#### token_info Object

| Field        | Type              | Description                                       |
| ------------ | ----------------- | ------------------------------------------------- |
| token_id     | String            | Token contract address                            |
| name         | String            | Token name                                        |
| symbol       | String            | Token symbol                                      |
| image_uri    | String            | Token image URL                                   |
| description  | String (nullable) | Token description                                 |
| is_graduated | Boolean           | Whether token graduated from bonding curve to DEX |
| is_nsfw      | Boolean           | Whether token is marked as NSFW content           |
| twitter      | String (nullable) | Twitter/X profile link                            |
| telegram     | String (nullable) | Telegram group link                               |
| website      | String (nullable) | Official website link                             |
| created_at   | Integer           | Creation timestamp (Unix timestamp in seconds)    |
| creator      | Object            | Creator account information                       |

#### creator Object

| Field      | Type   | Description                 |
| ---------- | ------ | --------------------------- |
| account_id | String | Creator's wallet address    |
| nickname   | String | Creator's display name      |
| bio        | String | Creator's biography         |
| image_uri  | String | Creator's profile image URL |

#### market_info Object

| Field          | Type    | Description                                                       |
| -------------- | ------- | ----------------------------------------------------------------- |
| market_type    | String  | Market type: "CURVE" (Bonding Curve) or "DEX" (Uniswap v3)        |
| token_id       | String  | Token contract address                                            |
| market_id      | String  | Market address (Bonding Curve address or Uniswap v3 pool address) |
| reserve_native | String  | Native token (MON) reserve in the pool                            |
| reserve_token  | String  | Project token reserve in the pool                                 |
| token_price    | String  | Token price in USD                                                |
| native_price   | String  | Native token (MON) price in USD                                   |
| price          | String  | Token price in native token (MON/Token)                           |
| total_supply   | String  | Total token supply                                                |
| volume         | String  | Trading volume                                                    |
| ath_price      | String  | All-time high price in native token                               |
| holder_count   | Integer | Number of unique token holders                                    |

#### Error Response (Status: 400)

```json
{
  "error": "Invalid token address"
}
```

---

# 2. Get Market Data API

### Basic Information

| Item        | Description                                    |
| ----------- | ---------------------------------------------- |
| URL         | /trade/market/:token_id                        |
| Method      | GET                                            |
| Description | Query current market data for a specific token |

### Parameters

| Parameter | Location | Type   | Required | Description                                               |
| --------- | -------- | ------ | -------- | --------------------------------------------------------- |
| token_id  | Path     | String | Required | EVM token address to query (42 characters with 0x prefix) |

### Response

#### Success Response (Status: 200)

```json
{
  "market_info": {
    "market_type": "CURVE",
    "token_id": "0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c",
    "market_id": "0xD5724171C2b7f0AA717a324626050BD05767e2C6",
    "reserve_native": "12345.67",
    "reserve_token": "987654321.0",
    "token_price": "0.00000028",
    "native_price": "1.25",
    "price": "2.8000E-8",
    "total_supply": "1000000000000000000000000000",
    "volume": "50000.0",
    "ath_price": "0.00000035",
    "holder_count": 142
  }
}
```

### Response Fields

| Field                      | Type    | Description                                                       |
| -------------------------- | ------- | ----------------------------------------------------------------- |
| market_info.market_type    | String  | Market type: "CURVE" (Bonding Curve) or "DEX" (Uniswap v3)        |
| market_info.token_id       | String  | Token contract address                                            |
| market_info.market_id      | String  | Market address (Bonding Curve address or Uniswap v3 pool address) |
| market_info.reserve_native | String  | Native token (MON) reserve in the pool                            |
| market_info.reserve_token  | String  | Project token reserve in the pool                                 |
| market_info.token_price    | String  | Token price in USD                                                |
| market_info.native_price   | String  | Native token (MON) price in USD                                   |
| market_info.price          | String  | Token price in native token (MON/Token)                           |
| market_info.total_supply   | String  | Total token supply                                                |
| market_info.volume         | String  | Trading volume                                                    |
| market_info.ath_price      | String  | All-time high price in native token                               |
| market_info.holder_count   | Integer | Number of unique token holders                                    |

#### Error Response (Status: 400)

```json
{
  "error": "Invalid token address"
}
```

---

# 3. Get Chart Data API

### Basic Information

| Item        | Description                                                                          |
| ----------- | ------------------------------------------------------------------------------------ |
| URL         | /trade/chart/:token_id                                                               |
| Method      | GET                                                                                  |
| Description | Query OHLCV chart data (candlesticks) for a specific token with multiple chart types |

### Parameters

| Parameter  | Location | Type    | Required | Description                                                                                 |
| ---------- | -------- | ------- | -------- | ------------------------------------------------------------------------------------------- |
| token_id   | Path     | String  | Required | EVM token address to query (42 characters with 0x prefix)                                   |
| resolution | Query    | String  | Required | Chart resolution: "1", "5", "15", "30", "60"/"1H", "240"/"4H", "D"/"1D", "W"/"1W", "M"/"1M" |
| from       | Query    | Integer | Required | Start timestamp (Unix timestamp in seconds)                                                 |
| to         | Query    | Integer | Required | End timestamp (Unix timestamp in seconds)                                                   |
| countback  | Query    | Integer | Optional | Maximum number of candles to return (default: 500)                                          |
| chart_type | Query    | String  | Optional | Chart type: "price", "price_usd", "market_cap", "market_cap_usd" (default: "price")         |

### Chart Types

| chart_type     | Description                                                  |
| -------------- | ------------------------------------------------------------ |
| price          | Token price in native token (MON/Token) - default            |
| price_usd      | Token price in USD                                           |
| market_cap     | Market capitalization in native token (price × total_supply) |
| market_cap_usd | Market capitalization in USD (usd_price × total_supply)      |

### Response

#### Success Response (Status: 200)

```json
{
  "s": "ok",
  "t": [1751460000, 1751460060, 1751460120],
  "o": ["0.00123", "0.00124", "0.00125"],
  "c": ["0.00124", "0.00125", "0.00126"],
  "h": ["0.00125", "0.00126", "0.00127"],
  "l": ["0.00122", "0.00123", "0.00124"],
  "v": ["1000.0", "1200.0", "1500.0"]
}
```

### Response Fields

| Field | Type           | Description                                                                       |
| ----- | -------------- | --------------------------------------------------------------------------------- |
| s     | String         | Status: "ok" (success), "no_data" (no candles found), or "error" (error occurred) |
| t     | Array[Integer] | Timestamps (Unix timestamps in seconds), sorted chronologically                   |
| o     | Array[String]  | Open prices for each candle                                                       |
| c     | Array[String]  | Close prices for each candle                                                      |
| h     | Array[String]  | High prices for each candle                                                       |
| l     | Array[String]  | Low prices for each candle                                                        |
| v     | Array[String]  | Trading volumes for each candle                                                   |

**Note:** All price/volume values are returned as strings to preserve decimal precision.

#### No Data Response (Status: 200)

```json
{
  "s": "no_data",
  "t": [],
  "o": [],
  "c": [],
  "h": [],
  "l": [],
  "v": []
}
```

#### Error Response (Status: 400)

```json
{
  "error": "Invalid token address"
}
```

### Resolution Format

| Input Value | Interval Type | Description |
| ----------- | ------------- | ----------- |
| 1           | 1             | 1 minute    |
| 5           | 5             | 5 minutes   |
| 15          | 15            | 15 minutes  |
| 30          | 30            | 30 minutes  |
| 60 or 1H    | 1H            | 1 hour      |
| 240 or 4H   | 4H            | 4 hours     |
| D or 1D     | D             | 1 day       |
| W or 1W     | W             | 1 week      |
| M or 1M     | M             | 1 month     |

---

# 4. Get Token Metrics API

### Basic Information

| Item        | Description                                                           |
| ----------- | --------------------------------------------------------------------- |
| URL         | /trade/metrics/:token_id                                              |
| Method      | GET                                                                   |
| Description | Query trading metrics for a specific token across multiple timeframes |

### Parameters

| Parameter  | Location | Type   | Required | Description                                                                             |
| ---------- | -------- | ------ | -------- | --------------------------------------------------------------------------------------- |
| token_id   | Path     | String | Required | EVM token address (42 characters with 0x prefix)                                        |
| timeframes | Query    | String | Required | Comma-separated timeframes: "1", "5", "15", "30", "60", "240", "1D" (e.g., "1,5,15,30") |

### Supported Timeframes

| Value | Display | Description |
| ----- | ------- | ----------- |
| 1     | 1m      | 1 minute    |
| 5     | 5m      | 5 minutes   |
| 15    | 15m     | 15 minutes  |
| 30    | 30m     | 30 minutes  |
| 60    | 1h      | 1 hour      |
| 240   | 4h      | 4 hours     |
| 1D    | 1d      | 1 day       |

### Response

#### Success Response (Status: 200)

```json
{
  "metrics": [
    {
      "timeframe": "1",
      "percent": 5.25,
      "transactions": {
        "buy": 150,
        "sell": 85,
        "total": 235
      },
      "volume": {
        "buy": "750000.50",
        "sell": "484567.39",
        "total": "1234567.89"
      },
      "makers": {
        "buy": 45,
        "sell": 32,
        "total": 77
      }
    },
    {
      "timeframe": "5",
      "percent": 12.8,
      "transactions": {
        "buy": 320,
        "sell": 210,
        "total": 530
      },
      "volume": {
        "buy": "1500000.00",
        "sell": "950000.00",
        "total": "2450000.00"
      },
      "makers": {
        "buy": 89,
        "sell": 67,
        "total": 156
      }
    }
  ]
}
```

### Response Fields

| Field                        | Type    | Description                                                    |
| ---------------------------- | ------- | -------------------------------------------------------------- |
| metrics                      | Array   | Array of metric objects, one for each requested timeframe      |
| metrics[].timeframe          | String  | Timeframe identifier ("1", "5", "15", "30", "60", "240", "1D") |
| metrics[].percent            | Float   | Price change percentage within the timeframe                   |
| metrics[].transactions       | Object  | Transaction count statistics                                   |
| metrics[].transactions.buy   | Integer | Number of buy transactions                                     |
| metrics[].transactions.sell  | Integer | Number of sell transactions                                    |
| metrics[].transactions.total | Integer | Total number of transactions (buy + sell)                      |
| metrics[].volume             | Object  | Trading volume statistics in native token (MON)                |
| metrics[].volume.buy         | String  | Buy volume in native token                                     |
| metrics[].volume.sell        | String  | Sell volume in native token                                    |
| metrics[].volume.total       | String  | Total volume (buy + sell)                                      |
| metrics[].makers             | Object  | Unique trader count statistics                                 |
| metrics[].makers.buy         | Integer | Number of unique buyers                                        |
| metrics[].makers.sell        | Integer | Number of unique sellers                                       |
| metrics[].makers.total       | Integer | Total unique traders                                           |

**Note:** All volume values are returned as strings to preserve decimal precision.

#### Error Responses

Invalid Token Address (Status: 400)

```json
{
  "error": "Invalid token ID"
}
```

No Timeframes Provided (Status: 400)

```json
{
  "error": "At least one timeframe must be provided"
}
```

Database Error (Status: 500)

```json
{
  "error": "Failed to get trading metrics: [error details]"
}
```

### Usage Examples

Query single timeframe:

```
GET /trade/metrics/0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c?timeframes=1
```

Query multiple timeframes:

```
GET /trade/metrics/0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c?timeframes=1,5,15,30,60,240,1D
```

---

# 5. Upload Image API

### Basic Information

| Item        | Description                                                |
| ----------- | ---------------------------------------------------------- |
| URL         | /metadata/image                                            |
| Method      | POST                                                       |
| Description | Upload an image with NSFW validation using AWS Rekognition |

### Request

**Content-Type:** image/jpeg, image/png, image/webp, or image/svg+xml

**Body:** Binary image data (raw bytes)

**File Size Limit:** 5MB

### Supported Image Formats

| Format | MIME Type     | Magic Bytes Validation                          |
| ------ | ------------- | ----------------------------------------------- |
| JPEG   | image/jpeg    | 0xFF, 0xD8, 0xFF                                |
| PNG    | image/png     | 0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A  |
| WebP   | image/webp    | RIFF header with WEBP at position 8             |
| SVG    | image/svg+xml | Starts with `<svg` or `<?xml` containing `<svg` |

### Validation Process

1. **File Format Verification:** Magic bytes check to verify actual file format
2. **Content-Type Validation:** Declared MIME type must match actual file format
3. **Image Conversion:** Images resized to max 1024x1024 for NSFW analysis, SVG rendered to PNG
4. **NSFW Detection:** AWS Rekognition analyzes for adult content with confidence thresholds

### Response

#### Success Response (Status: 200)

```json
{
  "is_nsfw": false,
  "image_uri": "https://storage.nadapp.net/coin/550e8400-e29b-41d4-a716-446655440000.png"
}
```

### Response Fields

| Field     | Type    | Description                                                               |
| --------- | ------- | ------------------------------------------------------------------------- |
| is_nsfw   | Boolean | Whether the image contains NSFW content based on AWS Rekognition analysis |
| image_uri | String  | Uploaded image URL on R2 storage CDN                                      |

#### Error Responses

Unsupported Image Format (Status: 400)

```json
{
  "error": "Unsupported image type: image/gif"
}
```

File Format Mismatch (Status: 400)

```json
{
  "error": "File format mismatch: declared image/png but actual image/jpeg"
}
```

File Too Large (Status: 413)

```json
{
  "error": "Payload too large"
}
```

NSFW Check Failed (Status: 500)

```json
{
  "error": "AWS Rekognition error: [error details]"
}
```

### Usage Examples

Upload JPEG image:

```bash
curl -X POST https://api.example.com/metadata/image \
  -H "Content-Type: image/jpeg" \
  --data-binary @image.jpg
```

Upload PNG image:

```bash
curl -X POST https://api.example.com/metadata/image \
  -H "Content-Type: image/png" \
  --data-binary @image.png
```

### NSFW Detection Categories

The following content categories are detected with respective confidence thresholds:

| Category                 | Minimum Confidence |
| ------------------------ | ------------------ |
| Explicit                 | 10.0%              |
| Explicit Nudity          | 10.0%              |
| Explicit Sexual Activity | 10.0%              |
| Exposed Buttocks or Anus | 10.0%              |
| Exposed Male Genitalia   | 10.0%              |
| Exposed Female Genitalia | 10.0%              |
| Exposed Female Nipple    | 10.0%              |
| Non-Explicit Nudity      | 90.0%              |

---

# 6. Upload Metadata API

### Basic Information

| Item        | Description                                      |
| ----------- | ------------------------------------------------ |
| URL         | /metadata/metadata                               |
| Method      | POST                                             |
| Description | Upload token metadata to R2 storage and database |

### Request

**Content-Type:** application/json

```json
{
  "image_uri": "https://storage.nadapp.net/coin/550e8400-e29b-41d4-a716-446655440000.png",
  "name": "Sample Token",
  "symbol": "SAMPLE",
  "description": "A sample token for demonstration purposes",
  "website": "https://example.com",
  "twitter": "https://x.com/example",
  "telegram": "https://t.me/example"
}
```

### Request Fields

| Field       | Type   | Required | Validation Rules                                          | Description                           |
| ----------- | ------ | -------- | --------------------------------------------------------- | ------------------------------------- |
| image_uri   | String | Yes      | Must be from https://storage.nadapp.net/, cannot be empty | Image URL from /metadata/image upload |
| name        | String | Yes      | Cannot be empty or whitespace                             | Token name                            |
| symbol      | String | Yes      | Cannot be empty or whitespace                             | Token symbol                          |
| description | String | Yes      | Cannot be empty or whitespace                             | Token description                     |
| website     | String | No       | Must start with https:// if provided                      | Website URL                           |
| twitter     | String | No       | Must contain x.com and start with https:// if provided    | X (Twitter) URL                       |
| telegram    | String | No       | Must contain t.me and start with https:// if provided     | Telegram URL                          |

### Response

#### Success Response (Status: 200)

```json
{
  "metadata_uri": "https://storage.nadapp.net/metadata/550e8400-e29b-41d4-a716-446655440000.json",
  "metadata": {
    "name": "Sample Token",
    "symbol": "SAMPLE",
    "description": "A sample token for demonstration purposes",
    "image_uri": "https://storage.nadapp.net/coin/550e8400-e29b-41d4-a716-446655440000.png",
    "website": "https://example.com",
    "twitter": "https://x.com/example",
    "telegram": "https://t.me/example",
    "is_nsfw": false
  }
}
```

### Response Fields

| Field                | Type              | Description                              |
| -------------------- | ----------------- | ---------------------------------------- |
| metadata_uri         | String            | Uploaded metadata JSON URL on R2 storage |
| metadata             | Object            | Complete token metadata object           |
| metadata.name        | String            | Token name                               |
| metadata.symbol      | String            | Token symbol                             |
| metadata.description | String            | Token description                        |
| metadata.image_uri   | String            | Token image URL                          |
| metadata.website     | String (nullable) | Website URL                              |
| metadata.twitter     | String (nullable) | X (Twitter) URL                          |
| metadata.telegram    | String (nullable) | Telegram URL                             |
| metadata.is_nsfw     | Boolean           | NSFW status inherited from image upload  |

#### Error Responses

Invalid Image URI (Status: 400)

```json
{
  "error": "Invalid image URI - must be from https://storage.nadapp.net/"
}
```

Empty Required Field (Status: 400)

```json
{
  "error": "Token name cannot be empty"
}
```

NSFW Status Not Found (Status: 400)

```json
{
  "error": "NSFW status not found for this image - please upload image first"
}
```

Invalid URL Format (Status: 400)

```json
{
  "error": "Invalid X (Twitter) URL format - must contain x.com"
}
```

Upload Failed (Status: 500)

```json
{
  "error": "Failed to upload metadata to R2"
}
```

### Important Notes

1. **Image Upload First:** You must upload the image via `/metadata/image` before uploading metadata. The NSFW status is cached for 3 minutes.
2. **HTTPS Only:** All URLs (website, twitter, telegram) must use HTTPS protocol for security.
3. **Domain Validation:** Twitter URLs must contain `x.com`, Telegram URLs must contain `t.me`.
4. **Caching:** NSFW status is cached in Redis for 3 minutes after image upload.
5. **Storage:** Metadata is stored both in R2 (as JSON file) and PostgreSQL database.

---

# 7. Mine Salt API

### Basic Information

| Item        | Description                                                                                  |
| ----------- | -------------------------------------------------------------------------------------------- |
| URL         | /token/salt                                                                                  |
| Method      | POST                                                                                         |
| Description | Generate a salt value to create a vanity token address (address ending with specific digits) |

### Request

**Content-Type:** application/json

```json
{
  "creator": "0x742d35Cc6634C0532925a3b844Bc9e7595f70143",
  "name": "My Token",
  "symbol": "MTK",
  "metadata_uri": "https://storage.nadapp.net/metadata-94a412d2-b599-4bb0-b026-b14c4036c58c.json"
}
```

### Request Fields

| Field        | Type   | Required | Description                                          |
| ------------ | ------ | -------- | ---------------------------------------------------- |
| creator      | String | Yes      | Creator's wallet address (EVM format with 0x prefix) |
| name         | String | Yes      | Token name (must match metadata)                     |
| symbol       | String | Yes      | Token symbol (must match metadata)                   |
| metadata_uri | String | Yes      | Metadata URI from Upload Metadata API                |

### Response

#### Success Response (Status: 200)

```json
{
  "salt": "0x000000000000000000000000000000000000000000000000000000000000a3f5",
  "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f70888"
}
```

### Response Fields

| Field   | Type   | Description                                                      |
| ------- | ------ | ---------------------------------------------------------------- |
| salt    | String | The mined salt value (32 bytes hex with 0x prefix)               |
| address | String | The resulting token address with desired suffix ending in "7777" |

### Algorithm Details

The salt mining process:

1. Generates random salt values
2. Computes the resulting token address using CREATE2
3. Checks if the address ends with "7777"
4. Returns the first salt that produces a matching address

**Note:** The algorithm searches for addresses ending with **8 consecutive 8s** ("7777").

#### Error Responses

Invalid Parameters (Status: 400)

```json
{
  "error": "Invalid creator address"
}
```

Request Timeout (Status: 408)

```json
{
  "error": "Max iterations reached",
  "iterations_attempted": 1000000
}
```

Internal Server Error (Status: 500)

```json
{
  "error": "Failed to mine salt: [error details]"
}
```

### Usage Examples

Mine salt for token creation:

```bash
curl -X POST https://api.nadapp.net/token/salt \
  -H "Content-Type: application/json" \
  -d '{
    "creator": "0x742d35Cc6634C0532925a3b844Bc9e7595f70143",
    "name": "My Token",
    "symbol": "MTK",
    "metadata_uri": "https://storage.nadapp.net/metadata-94a412d2-b599-4bb0-b026-b14c4036c58c.json"
  }'
```

### Important Notes

1. **Vanity Address:** The salt mining process generates vanity addresses ending with "7777" (8 consecutive 8s)
2. **Processing Time:** May take time depending on computational complexity and randomness
3. **Timeout Limit:** Has a maximum iteration limit to prevent infinite loops
4. **Smart Contract Integration:** Use the returned `salt` and `address` values when deploying the token contract via CREATE2
5. **Deterministic:** Given the same inputs (creator, name, symbol, metadata_uri) and salt, the resulting address is deterministic

---

## Changelog

### Latest Updates (2025-01-20)

#### All APIs

- All numeric values (prices, volumes, supplies) are now returned as strings to preserve decimal precision
- All endpoints use EVM address validation (42 characters with 0x prefix)

#### Token Metadata API (`/token/metadata/:token_id`)

- **Added** `token_info` and `market_info` nested structure
- **Added** `creator` object with account details (account_id, nickname, bio, image_uri)
- **Added** `is_graduated` field to indicate DEX listing status
- **Added** `is_nsfw` field to mark NSFW content
- **Added** market data fields: `reserve_native`, `reserve_token`, `token_price`, `native_price`, `volume`, `ath_price`, `holder_count`
- **Removed** `transaction_hash` field from response

#### Market Data API (`/trade/market/:token_id`)

- **Added** `reserve_native` field (native token reserve in pool)
- **Added** `reserve_token` field (project token reserve in pool)
- **Added** `token_price` field (USD price)
- **Added** `native_price` field (MON/USD price)
- **Added** `volume`, `ath_price`, and `holder_count` fields
- **Removed** `liquidity` field (replaced by reserve fields)

#### Chart Data API (`/trade/chart/:token_id`)

- **Added** `chart_type` query parameter with 4 options: price, price_usd, market_cap, market_cap_usd
- **Added** support for monthly resolution ("M" or "1M")
- Arrays are now guaranteed to be sorted chronologically (oldest to newest)
- Clarified that response arrays use string format for decimal precision

#### Token Metrics API (`/trade/metrics/:token_id`)

- **Changed** from single `timeframe` parameter to `timeframes` (comma-separated) for batch requests
- **Removed** support for weekly ("W") and monthly ("M") timeframes - now supports: 1, 5, 15, 30, 60, 240, 1D
- **Changed** response structure to array of metric objects (one per timeframe)
- **Added** detailed breakdown: transactions (buy/sell/total), volume (buy/sell/total), makers (buy/sell/total)
- **Removed** individual fields like `buy_count`, `sell_count`, `current_price`, `start_price`
- **Added** `percent` field for price change percentage
- Uses `price_history` table instead of chart table for price calculations
- Implements fallback logic when no price exists at timeframe start

#### Upload Image API (`/metadata/image`)

- **Added** magic bytes validation for file format verification
- **Added** SVG support with automatic PNG conversion for NSFW analysis
- **Added** detailed NSFW detection categories with specific confidence thresholds
- **Changed** file size limit to 5MB
- Images automatically resized to 1024x1024 for NSFW analysis
- NSFW status cached in Redis for 3 minutes

#### Upload Metadata API (`/metadata/metadata`)

- **Added** strict validation for required fields (name, symbol, description cannot be empty)
- **Added** domain validation for image_uri (must be from storage.nadapp.net)
- **Added** URL format validation (HTTPS only, specific domain requirements)
- **Changed** Twitter validation to require `x.com` domain
- **Added** NSFW status inheritance from image upload
- Metadata stored in both R2 storage and PostgreSQL database
