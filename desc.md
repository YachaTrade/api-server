# API Documentation

## URL : "https://testnet-api-server.nad.fun/"

## 1. Get Token Information API (`get_token`)

### Basic Information

| Item            | Description                                      |
| --------------- | ------------------------------------------------ |
| **URL**         | `/bot/token/{token_address}`                     |
| **Method**      | GET                                              |
| **Description** | Query token metadata information via EVM address |

### Parameters

| Parameter       | Location | Type   | Required | Description                                               |
| --------------- | -------- | ------ | -------- | --------------------------------------------------------- |
| `token_address` | Path     | String | Required | EVM token address to query (42 characters with 0x prefix) |

### Response

#### Success Response (Status: 200)

```json
{
  "token_address": "0x9569ad4B353D4811064ad9970B198fcb914428D5",
  "name": "pill.NADS ⨀",
  "symbol": "pillNADS",
  "image_uri": "https://storage.nadapp.net/coin/7bac3557-8528-4cbb-9c58-e91d098d28ad",
  "description": "pill.NADS ⨀",
  "twitter": "",
  "telegram": "https://t.me/monadvietnam",
  "website": "",
  "is_listing": true,
  "created_at": 1743568715,
  "transaction_hash": "0x90825214f6d1c1cb80d8d5c217a8c26c75293e4c76b63c73c91f2f8eb858c558",
  "creator": "0x93a7c39d7f848A1e9C479C6FE1F8995015Ea2fb9",
  "total_supply": "997552313017378376186352266"
}
```

#### Error Response (Status: 400)

```json
{
  "error": "Invalid token address"
}
```

#### Error Response (Status: 404)

```json
{
  "error": "Failed to get token: token_address: 0x2d3470614E99A0485995d682f512Dc2b589e36e, error: Token not found"
}
```

### Error Codes

| Status Code | Description                                |
| ----------- | ------------------------------------------ |
| 400         | Bad Request - Invalid token address format |
| 404         | Token not found                            |
| 500         | Server error                               |

### Implementation Details

- Uses Redis caching: Default TTL 30 seconds
- Falls back to DB query if cache fails
- Asynchronous caching for response time optimization

## 2. Get Chart Data API (`get_prices`)

### Basic Information

| Item            | Description                                     |
| --------------- | ----------------------------------------------- |
| **URL**         | `/bot/prices/{token_address}`                   |
| **Method**      | GET                                             |
| **Description** | Query chart data (candles) for a specific token |

### Parameters

| Parameter       | Location | Type    | Required | Description                                               |
| --------------- | -------- | ------- | -------- | --------------------------------------------------------- |
| `token_address` | Path     | String  | Required | EVM token address to query (42 characters with 0x prefix) |
| `resolution`    | Query    | String  | Required | Chart resolution (1, 5, 15, 30, 60/1H, 4H, D, W)          |
| `from`          | Query    | Integer | Required | Start timestamp (in seconds)                              |
| `to`            | Query    | Integer | Required | End timestamp (in seconds)                                |
| `countback`     | Query    | Integer | Optional | Maximum number of candles to return (default: 500)        |

### Response

#### Success Response (Status: 200)

```json
{
  "s": "ok",
  "t": [1751460000, 1751460060, 1751460120],
  "o": [0.00123, 0.00124, 0.00125],
  "c": [0.00124, 0.00125, 0.00126],
  "h": [0.00125, 0.00126, 0.00127],
  "l": [0.00122, 0.00123, 0.00124],
  "v": [1000, 1200, 1500]
}
```

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

#### Error Response (Status: 500)

```json
{
  "error": "Failed to get price chart data: token_id: 0x2d3470614E99A0485995d682f512Dc2b589e36e, error: Database connection error"
}
```

### Error Codes

| Status Code | Description                                              |
| ----------- | -------------------------------------------------------- |
| 400         | Bad Request - Invalid token address format or parameters |
| 500         | Server error                                             |

### Validation

- Token address must be a 42-character EVM address format starting with 0x
- Resolution value must be one of the specified values (1, 5, 15, 30, 60/1H, 4H, D, W)
- Timestamps must be in seconds

### Testing Methods

Test with valid token address:

```bash
curl -X 'GET' \
  'https://testnet-api-server.nad.fun/bot/token/0xb7989D1b543A9bDD5251ae2417b7700F2F654aA0' \
  -H 'accept: application/json'

curl -X 'GET' \
  'https://testnet-api-server.nad.fun/bot/prices/0xb7989D1b543A9bDD5251ae2417b7700F2F654aA0?resolution=1&from=1751460000&to=1751470000&countback=100' \
  -H 'accept: application/json'
```

### Data Type Definitions

#### TokenBotResponse Structure

```rust
pub struct TokenBotResponse {
    pub token_id: String,           // Token address
    pub token_name: Option<String>, // Token name
    pub token_symbol: Option<String>, // Token symbol
    pub decimals: Option<i32>,      // Decimal places
    pub contract_type: Option<String>, // Contract type (e.g., ERC20)
    pub website: Option<String>,    // Website URL
    pub is_listing: bool,           // Listing status
    pub created_at: Option<String>, // Creation time
    pub transaction_hash: Option<String>, // Creation transaction hash
    pub creator: Option<String>,    // Creator address
    pub total_supply: Option<String>, // Total supply (returned as string)
}
```

#### BarResponse Structure

```rust
pub struct BarResponse {
    pub s: String,           // Status ("ok" or "no_data")
    pub t: Vec<i64>,         // Timestamp array
    pub o: Vec<String>,      // Open price array
    pub h: Vec<String>,      // High price array
    pub l: Vec<String>,      // Low price array
    pub c: Vec<String>,      // Close price array
    pub v: Vec<String>,      // Volume array
}
```
