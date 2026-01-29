# Account API 문서

## 개요

Account API는 사용자 계정 관리를 위한 API입니다.

- **계정 조회/수정**: 닉네임, 바이오, 프로필 이미지 관리
- **X(Twitter) 연동**: X 계정 연결/해제/업데이트
- **지갑 관리**: 지갑 타입 등록 및 조회
- **이미지 업로드**: 프로필 이미지 업로드 (R2 스토리지)

모든 API는 **세션 인증 필수** (session cookie)

---

## API 엔드포인트

### 1. 계정 조회 (`GET /account/get_account`)

현재 로그인한 사용자의 계정 정보를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 필수 (session cookie)

#### 응답
```json
{
  "account_info": {
    "account_id": "0x1234567890abcdef...",
    "nickname": "MyNickname",
    "bio": "Hello, I'm a trader!",
    "image_uri": "https://storage.nadapp.net/accounts/uuid.png"
  }
}
```

#### 에러 응답
- `401`: 인증 실패 (세션 없음/만료)
- `500`: 내부 서버 에러

---

### 2. 계정 수정 (`PATCH /account/update`)

계정 프로필 정보를 수정합니다. 모든 필드는 선택적입니다.

#### 요청
- **Method**: `PATCH`
- **Content-Type**: `application/json`
- **인증**: 필수 (session cookie)

#### Request Body
```json
{
  "nickname": "NewNickname",
  "bio": "Updated bio text",
  "image_uri": "https://storage.nadapp.net/accounts/new-uuid.png"
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `nickname` | string | X | 닉네임 (1-15자, `@` `#`로 시작 불가) |
| `bio` | string | X | 자기소개 (최대 200자) |
| `image_uri` | string | X | 프로필 이미지 URL (최대 500자) |

#### 응답
```json
{
  "account_info": {
    "account_id": "0x1234567890abcdef...",
    "nickname": "NewNickname",
    "bio": "Updated bio text",
    "image_uri": "https://storage.nadapp.net/accounts/new-uuid.png"
  }
}
```

#### 에러 응답
- `400`: 유효성 검사 실패 (닉네임 길이 초과, bio 길이 초과 등)
- `401`: 인증 실패
- `500`: 내부 서버 에러

---

### 3. X 계정 연결 (`PUT /account/connect_x`)

X(Twitter) 계정을 연결합니다.

#### 요청
- **Method**: `PUT`
- **Content-Type**: `application/json`
- **인증**: 필수 (session cookie)

#### Request Body
```json
{
  "is_blue_label": true,
  "x_handle": "@username",
  "x_image_uri": "https://pbs.twimg.com/profile_images/..."
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `is_blue_label` | boolean | O | 블루 라벨(인증) 여부 |
| `x_handle` | string | O | X 핸들 (최대 100자) |
| `x_image_uri` | string | O | X 프로필 이미지 URL (최대 500자) |

#### 응답
```json
{
  "account_info": {
    "account_id": "0x1234567890abcdef...",
    "nickname": "MyNickname",
    "bio": "Hello!",
    "image_uri": "https://..."
  }
}
```

#### 에러 응답
- `400`: 유효성 검사 실패 (핸들/이미지 URL 길이 초과)
- `401`: 인증 실패
- `500`: 내부 서버 에러

---

### 4. X 계정 연결 해제 (`DELETE /account/disconnect_x`)

X(Twitter) 계정 연결을 해제합니다.

#### 요청
- **Method**: `DELETE`
- **인증**: 필수 (session cookie)

#### 응답
```json
{
  "account_info": {
    "account_id": "0x1234567890abcdef...",
    "nickname": "MyNickname",
    "bio": "Hello!",
    "image_uri": "https://..."
  }
}
```

#### 에러 응답
- `401`: 인증 실패
- `500`: 내부 서버 에러

---

### 5. X 계정 이미지 업데이트 (`PATCH /account/update_x`)

연결된 X 계정의 프로필 이미지를 업데이트합니다.

#### 요청
- **Method**: `PATCH`
- **Content-Type**: `application/json`
- **인증**: 필수 (session cookie)

#### Request Body
```json
{
  "x_image_uri": "https://pbs.twimg.com/profile_images/new-image..."
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `x_image_uri` | string | O | X 프로필 이미지 URL (최대 500자) |

#### 응답
```json
{
  "account_info": {
    "account_id": "0x1234567890abcdef...",
    "nickname": "MyNickname",
    "bio": "Hello!",
    "image_uri": "https://..."
  }
}
```

#### 에러 응답
- `400`: 유효성 검사 실패 (이미지 URL 길이 초과)
- `401`: 인증 실패
- `500`: 내부 서버 에러

---

### 6. 지갑 등록 (`PATCH /account/register_wallet`)

사용자의 지갑 타입을 등록합니다.

#### 요청
- **Method**: `PATCH`
- **Content-Type**: `application/json`
- **인증**: 필수 (session cookie)

#### Request Body
```json
{
  "wallet": "METAMASK"
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `wallet` | string | O | 지갑 타입 |

#### 허용되는 지갑 타입
- `METAMASK`
- `KEPLR`
- `BACKPACK`
- `HAHA`
- `OKX`
- `PHANTOM`
- `RABBY`
- `OTHER`

#### 응답
```json
{
  "account_info": {
    "account_id": "0x1234567890abcdef...",
    "nickname": "MyNickname",
    "bio": "Hello!",
    "image_uri": "https://..."
  }
}
```

#### 에러 응답
- `400`: 유효성 검사 실패 (허용되지 않은 지갑 타입)
- `401`: 인증 실패
- `500`: 내부 서버 에러

---

### 7. 지갑 조회 (`GET /account/wallet`)

등록된 지갑 정보를 조회합니다.

#### 요청
- **Method**: `GET`
- **인증**: 필수 (session cookie)

#### 응답
```json
{
  "account_id": "0x1234567890abcdef...",
  "wallet": "METAMASK"
}
```

#### 에러 응답
- `401`: 인증 실패
- `500`: 내부 서버 에러

---

### 8. 프로필 이미지 업로드 (`POST /account/image`)

프로필 이미지를 R2 스토리지에 업로드합니다.

#### 요청
- **Method**: `POST`
- **Content-Type**: `image/jpeg`, `image/jpg`, `image/png`, `image/gif`, `image/webp`, `image/heic`
- **Body**: Raw 이미지 바이너리 데이터

#### 제약 조건
- **최대 파일 크기**: 5MB
- **허용 이미지 타입**: JPEG, JPG, PNG, GIF, WEBP, HEIC

#### 응답
```json
{
  "image_uri": "https://storage.nadapp.net/accounts/uuid.png"
}
```

#### 에러 응답
- `400`: 잘못된 요청 (이미지 형식 오류, Content-Type 헤더 누락)
- `413`: 파일 크기 초과 (5MB 초과)
- `500`: 업로드 실패 (R2 에러)

---

## TypeScript Interfaces

### 요청 타입

```typescript
// PATCH /account/update
interface UpdateAccountRequest {
  nickname?: string;   // 1-15자, @/#로 시작 불가
  bio?: string;        // 최대 200자
  image_uri?: string;  // 최대 500자
}

// PUT /account/connect_x
interface ConnectXRequest {
  is_blue_label: boolean;
  x_handle: string;     // 최대 100자
  x_image_uri: string;  // 최대 500자
}

// PATCH /account/update_x
interface UpdateXRequest {
  x_image_uri: string;  // 최대 500자
}

// PATCH /account/register_wallet
interface RegisterWalletRequest {
  wallet: WalletType;
}

type WalletType =
  | "METAMASK"
  | "KEPLR"
  | "BACKPACK"
  | "HAHA"
  | "OKX"
  | "PHANTOM"
  | "RABBY"
  | "OTHER";
```

### 응답 타입

```typescript
// 공통 응답 (대부분의 Account API에서 사용)
interface AccountResponse {
  account_info: AccountInfo;
}

interface AccountInfo {
  account_id: string;  // EVM 지갑 주소
  nickname: string;
  bio: string;
  image_uri: string;
}

// GET /account/wallet
interface GetWalletResponse {
  account_id: string;
  wallet: string;
}

// POST /account/image
interface UploadImageResponse {
  image_uri: string;
}
```

---

## 유효성 검사 규칙

### 닉네임 (nickname)
- 최소 길이: 1자
- 최대 길이: 15자
- `@` 또는 `#`로 시작할 수 없음

### 바이오 (bio)
- 최대 길이: 200자

### 이미지 URI (image_uri, x_image_uri)
- 최대 길이: 500자

### X 핸들 (x_handle)
- 최대 길이: 100자

### 프로필 이미지 업로드
- 최대 파일 크기: 5MB
- 허용 Content-Type:
  - `image/jpeg`
  - `image/jpg`
  - `image/png`
  - `image/gif`
  - `image/webp`
  - `image/heic`
