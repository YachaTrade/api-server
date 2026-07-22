# Token Creation Flow

## 개요

토큰 생성은 3단계로 진행됩니다:

1. **이미지 업로드** (`POST /metadata/image`) — 토큰 이미지 업로드 + NSFW 검사
2. **메타데이터 업로드** (`POST /metadata/metadata`) — 토큰 메타데이터 JSON 생성 + R2 업로드
3. **Salt 마이닝** (`POST /token/salt`) — vanity address(7777 접미사) 생성을 위한 salt 계산

이후 클라이언트가 salt와 metadata_uri를 사용하여 온체인 토큰 생성 트랜잭션을 실행합니다.

---

## Step 1. 이미지 업로드 (`POST /metadata/image`)

토큰 프로필 이미지를 업로드합니다. NSFW 검사를 자동 수행합니다.

### 요청

- **Method**: `POST`
- **Content-Type**: `image/png`, `image/jpeg`, `image/webp`, `image/svg+xml`
- **Body**: Raw binary image data
- **Size Limit**: 5MB

### 응답

```json
{
  "is_nsfw": false,
  "image_uri": "https://storage.nadapp.net/images/uuid.png"
}
```

### 필드 설명

| 필드 | 타입 | 설명 |
|------|------|------|
| `is_nsfw` | boolean | NSFW 판정 결과 |
| `image_uri` | string | R2에 업로드된 이미지 URL |

### 에러 응답

| 상태 | 설명 |
|------|------|
| 400 | 잘못된 이미지 포맷 또는 이미지 없음 |
| 413 | 이미지 크기 5MB 초과 |
| 500 | NSFW 검사 실패 또는 업로드 실패 |

---

## Step 2. 메타데이터 업로드 (`POST /metadata/metadata`)

Step 1에서 받은 `image_uri`를 포함하여 토큰 메타데이터를 생성합니다.

### 요청

- **Method**: `POST`
- **Content-Type**: `application/json`

```json
{
  "image_uri": "https://storage.nadapp.net/images/uuid.png",
  "name": "My Token",
  "symbol": "MTK",
  "description": "Token description",
  "website": "https://mytoken.com",
  "twitter": "https://x.com/mytoken",
  "telegram": "https://t.me/mytoken"
}
```

### 필드 설명

| 필드 | 타입 | 필수 | 검증 규칙 |
|------|------|------|-----------|
| `image_uri` | string | O | `https://storage.nadapp.net/`로 시작해야 함 |
| `name` | string | O | 1-32자, 줄바꿈 불가 |
| `symbol` | string | O | 1-10자, 영숫자만 허용 |
| `description` | string | X | 최대 500자 |
| `website` | string | X | `https://`로 시작 |
| `twitter` | string | X | `https://x.com/`으로 시작 |
| `telegram` | string | X | `https://t.me/`로 시작 |

### 응답

```json
{
  "metadata_uri": "https://storage.nadapp.net/metadata-94a412d2-b599-4bb0-b026-b14c4036c58c.json",
  "metadata": {
    "name": "My Token",
    "symbol": "MTK",
    "description": "Token description",
    "image_uri": "https://storage.nadapp.net/images/uuid.png",
    "website": "https://mytoken.com",
    "twitter": "https://x.com/mytoken",
    "telegram": "https://t.me/mytoken",
    "is_nsfw": false
  }
}
```

### 에러 응답

| 상태 | 설명 |
|------|------|
| 400 | 유효성 검사 실패 또는 NSFW 상태 미확인 이미지 |
| 500 | R2 또는 DB 업로드 실패 |

---

## Step 3. Salt 마이닝 (`POST /token/salt`)

토큰 주소가 `7777`로 끝나는 vanity address를 생성하기 위한 salt를 계산합니다.

### 요청

- **Method**: `POST`
- **Content-Type**: `application/json`

```json
{
  "creator": "0x742d35Cc6634C0532925a3b844Bc9e7595f70143",
  "name": "My Token",
  "symbol": "MTK",
  "metadata_uri": "https://storage.nadapp.net/metadata-94a412d2-b599-4bb0-b026-b14c4036c58c.json"
}
```

### 필드 설명

| 필드 | 타입 | 필수 | 검증 규칙 |
|------|------|------|-----------|
| `creator` | string | O | 유효한 EVM 주소 (EIP-55 checksum) |
| `name` | string | O | 1-32자, 줄바꿈 불가 |
| `symbol` | string | O | 1-10자, 영숫자만 |
| `metadata_uri` | string | O | `https://storage.nadapp.net/`로 시작 |

### 응답

```json
{
  "salt": "0x000000000000000000000000000000000000000000000000000000000000a3f5",
  "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f7777"
}
```

### 에러 응답

| 상태 | 설명 |
|------|------|
| 400 | 유효성 검사 실패 |
| 408 | 타임아웃 (최대 반복 횟수 도달) |
| 500 | 내부 서버 에러 |

---

## 전체 플로우 요약

```
Client                          API Server                      R2 Storage
  |                                |                                |
  |  1. POST /metadata/image       |                                |
  |  (raw image binary)            |                                |
  |------------------------------->|                                |
  |                                |-- NSFW 검사                    |
  |                                |-- 이미지 업로드 --------------->|
  |  { image_uri, is_nsfw }        |                                |
  |<-------------------------------|                                |
  |                                |                                |
  |  2. POST /metadata/metadata    |                                |
  |  { image_uri, name, symbol }   |                                |
  |------------------------------->|                                |
  |                                |-- 유효성 검사                  |
  |                                |-- metadata JSON 업로드 ------->|
  |  { metadata_uri, metadata }    |                                |
  |<-------------------------------|                                |
  |                                |                                |
  |  3. POST /token/salt           |                                |
  |  { creator, name, symbol,      |                                |
  |    metadata_uri }              |                                |
  |------------------------------->|                                |
  |                                |-- salt 연산 (vanity address)   |
  |  { salt, address }             |                                |
  |<-------------------------------|                                |
  |                                |                                |
  |  4. 온체인 토큰 생성 TX         |                                |
  |  (salt + metadata_uri 사용)     |                                |
  |------------------------------->| Blockchain                     |
```

---

## TypeScript Interfaces

```typescript
// Step 1: POST /metadata/image
// Request: raw binary (Content-Type: image/png 등)

interface UploadImageResponse {
  is_nsfw: boolean;
  image_uri: string;
}

// Step 2: POST /metadata/metadata
interface UploadMetadataRequest {
  image_uri: string;
  name: string;
  symbol: string;
  description?: string;
  website?: string;
  twitter?: string;
  telegram?: string;
}

interface UploadMetadataResponse {
  metadata_uri: string;
  metadata: TokenMetadata;
}

interface TokenMetadata {
  name: string;
  symbol: string;
  description?: string;
  image_uri: string;
  website?: string;
  twitter?: string;
  telegram?: string;
  is_nsfw: boolean;
}

// Step 3: POST /token/salt
interface MineSaltRequest {
  creator: string;
  name: string;
  symbol: string;
  metadata_uri: string;
}

interface MineSaltResponse {
  salt: string;
  address: string;
}

interface MineSaltError {
  error: string;
  iterations_attempted?: number;
}
```
