# Agent API Live Testing Plan

## Overview
Agent API의 **10개 엔드포인트**에 대한 실제 curl 호출 테스트 계획입니다.

## Pre-requisites

```bash
# Environment
export BASE_URL="https://dev-api.nad.fun"  # Testnet
export API_KEY="your-api-key-here"

# Test Data
export TOKEN_ID="0xF716AE57Ce5fAf803D021c81E2Bbe1AD622fE85c"
export ACCOUNT_ID="0x9b834355d9EbcDFb291eAba6809B9E8D2C6b88d1"
export INVALID_TOKEN="0xinvalid"
export INVALID_ACCOUNT="not-an-address"
```

## Test Execution Order

1. **Authentication Tests** - API key 검증
2. **GET Endpoints** - 읽기 작업 (부작용 없음)
3. **POST Endpoints** - 쓰기 작업 (데이터 생성)

---

## Phase 1: Authentication Tests

### 1.1 Missing API Key (401)
```bash
curl -s -w "\nHTTP: %{http_code}\n" \
  "${BASE_URL}/agent/token/${TOKEN_ID}"
# Expected: 401
```

### 1.2 Invalid API Key (401)
```bash
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: invalid-key" \
  "${BASE_URL}/agent/token/${TOKEN_ID}"
# Expected: 401
```

---

## Phase 2: Trading Data Endpoints

### 2.1 GET /agent/chart/{token_id}

```bash
FROM_TS=$(($(date +%s) - 86400))
TO_TS=$(date +%s)

# Success
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/chart/${TOKEN_ID}?resolution=5&from=${FROM_TS}&to=${TO_TS}"

# Invalid token (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/chart/${INVALID_TOKEN}?resolution=5&from=${FROM_TS}&to=${TO_TS}"

# Invalid resolution (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/chart/${TOKEN_ID}?resolution=999&from=${FROM_TS}&to=${TO_TS}"
```

### 2.2 GET /agent/swap-history/{token_id}

```bash
# Success
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/swap-history/${TOKEN_ID}"

# With filters
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/swap-history/${TOKEN_ID}?page=1&limit=5&trade_type=BUY"

# Invalid token (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/swap-history/${INVALID_TOKEN}"
```

### 2.3 GET /agent/market/{token_id}

```bash
# Success
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/market/${TOKEN_ID}"

# Invalid token (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/market/${INVALID_TOKEN}"
```

### 2.4 GET /agent/metrics/{token_id}

```bash
# Success - multiple timeframes
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/metrics/${TOKEN_ID}?timeframes=1,5,60,1D"

# Invalid timeframe (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/metrics/${TOKEN_ID}?timeframes=999"
```

---

## Phase 3: Token Data Endpoints

### 3.1 GET /agent/token/{token_id}

```bash
# Success
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/token/${TOKEN_ID}"

# Invalid token (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/token/${INVALID_TOKEN}"
```

---

## Phase 4: Holdings Endpoint

### 4.1 GET /agent/holdings/{account_id}

```bash
# Success
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/holdings/${ACCOUNT_ID}"

# With pagination
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/holdings/${ACCOUNT_ID}?page=1&limit=5"

# Invalid account (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/holdings/${INVALID_ACCOUNT}"
```

---

## Phase 5: Token Creation Endpoints

### 5.1 GET /agent/token/created/{account_id}

```bash
# Success
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/token/created/${ACCOUNT_ID}"

# Invalid account (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  "${BASE_URL}/agent/token/created/${INVALID_ACCOUNT}"
```

### 5.2 POST /agent/token/image

```bash
# Create test image
curl -s -o /tmp/test.png "https://via.placeholder.com/100x100.png"

# Success
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  -H "Content-Type: image/png" \
  --data-binary @/tmp/test.png \
  "${BASE_URL}/agent/token/image"

# Empty body (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  -H "Content-Type: image/png" \
  -d "" \
  "${BASE_URL}/agent/token/image"
```

### 5.3 POST /agent/token/metadata

```bash
# First get image_uri from image upload
IMAGE_URI=$(curl -s \
  -H "X-API-Key: ${API_KEY}" \
  -H "Content-Type: image/png" \
  --data-binary @/tmp/test.png \
  "${BASE_URL}/agent/token/image" | jq -r '.image_uri')

# Success
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  -H "Content-Type: application/json" \
  -d "{
    \"image_uri\": \"${IMAGE_URI}\",
    \"name\": \"Test Token\",
    \"symbol\": \"TEST\"
  }" \
  "${BASE_URL}/agent/token/metadata"

# Invalid image_uri domain (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  -H "Content-Type: application/json" \
  -d '{
    "image_uri": "https://evil.com/image.png",
    "name": "Test",
    "symbol": "TEST"
  }' \
  "${BASE_URL}/agent/token/metadata"

# Name too long (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  -H "Content-Type: application/json" \
  -d "{
    \"image_uri\": \"${IMAGE_URI}\",
    \"name\": \"This name is way too long and exceeds 32 characters\",
    \"symbol\": \"TEST\"
  }" \
  "${BASE_URL}/agent/token/metadata"
```

### 5.4 POST /agent/salt

```bash
# Success (may take several seconds)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  -H "Content-Type: application/json" \
  -d "{
    \"creator\": \"${ACCOUNT_ID}\",
    \"name\": \"Test Token\",
    \"symbol\": \"TEST\",
    \"metadata_uri\": \"https://storage.nadapp.net/metadata-test.json\"
  }" \
  "${BASE_URL}/agent/salt"

# Invalid creator (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  -H "Content-Type: application/json" \
  -d '{
    "creator": "invalid",
    "name": "Test",
    "symbol": "TEST",
    "metadata_uri": "https://storage.nadapp.net/test.json"
  }' \
  "${BASE_URL}/agent/salt"

# Invalid metadata_uri domain (400)
curl -s -w "\nHTTP: %{http_code}\n" \
  -H "X-API-Key: ${API_KEY}" \
  -H "Content-Type: application/json" \
  -d "{
    \"creator\": \"${ACCOUNT_ID}\",
    \"name\": \"Test\",
    \"symbol\": \"TEST\",
    \"metadata_uri\": \"https://evil.com/test.json\"
  }" \
  "${BASE_URL}/agent/salt"
```

---

## Pass/Fail Criteria

| Criteria | Pass | Fail |
|----------|------|------|
| Success tests | HTTP 200 | Other status |
| Auth tests | HTTP 401 | Other status |
| Validation tests | HTTP 400 | Other status |
| Response format | Valid JSON | Malformed |
| Timeout | < 30s (120s for salt) | Exceeded |

## Test Summary

| Endpoint | Success | Error Cases |
|----------|---------|-------------|
| chart | 1 | 2 (invalid token, resolution) |
| swap-history | 2 | 1 (invalid token) |
| market | 1 | 1 (invalid token) |
| metrics | 1 | 1 (invalid timeframe) |
| token | 1 | 1 (invalid token) |
| holdings | 2 | 1 (invalid account) |
| token/created | 1 | 1 (invalid account) |
| token/image | 1 | 1 (empty body) |
| token/metadata | 1 | 2 (invalid uri, name) |
| salt | 1 | 2 (invalid creator, uri) |
| **Total** | **13** | **13** |
