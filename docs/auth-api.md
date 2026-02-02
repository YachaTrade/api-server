# Auth API 문서

## 개요

Auth API는 EVM 지갑 기반 인증을 위한 API입니다. SIWE(Sign-In with Ethereum) 방식을 사용하여 지갑 서명으로 사용자를 인증합니다.

- **Nonce 생성**: 서명 검증을 위한 일회용 nonce 발급
- **세션 생성**: 서명 검증 후 인증 세션 생성 및 쿠키 설정
- **세션 삭제**: 로그아웃 처리 (세션 무효화)

---

## API 엔드포인트

### 1. Nonce 생성 (`POST /auth/nonce`)

서명 검증을 위한 일회용 nonce를 생성합니다.

#### 요청
- **Method**: `POST`
- **Content-Type**: `application/json`
- **인증**: 불필요

#### Request Body
```json
{
  "address": "0x1234567890abcdef1234567890abcdef12345678"
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `address` | string | O | EVM 지갑 주소 (0x로 시작하는 40자리 hex) |

#### 검증 규칙
- `address`는 유효한 EVM 주소 형식이어야 함

#### 응답
```json
{
  "nonce": "Sign this message to authenticate: abc123xyz..."
}
```

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 주소 형식)
- `500`: 내부 서버 에러

---

### 2. 세션 생성 (`POST /auth/session`)

서명을 검증하고 인증 세션을 생성합니다. 성공 시 HTTP-only 쿠키에 세션 ID가 설정됩니다.

#### 요청
- **Method**: `POST`
- **Content-Type**: `application/json`
- **인증**: 불필요

#### Request Body
```json
{
  "signature": "0x1234567890abcdef...",
  "nonce": "Sign this message to authenticate: abc123xyz...",
  "chain_id": 10143,
  "wallet_address": "0x1234567890abcdef1234567890abcdef12345678"
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `signature` | string | O | ECDSA 서명 (0x + 130자리 hex, 총 132자) |
| `nonce` | string | O | `/auth/nonce`에서 받은 nonce 값 |
| `chain_id` | number | O | 체인 ID (예: 10143 for Monad) |
| `wallet_address` | string | X | 스마트 지갑 주소 (옵션, EIP-1271 검증용) |

#### 검증 규칙
- `signature`는 `0x`로 시작하고 132자(65바이트 ECDSA 서명)
- `signature`는 16진수 문자만 포함
- `nonce`는 1~256자 사이
- `wallet_address`가 있으면 유효한 EVM 주소 형식이어야 함

#### 처리 과정

1. 요청 검증
2. Nonce 유효성 확인 (Redis에서 조회)
3. 서명 검증 (ECDSA 복구 또는 EIP-1271)
4. 계정 조회 또는 생성
5. 세션 생성 및 Redis 저장
6. HTTP-only 쿠키 설정

#### 응답 헤더
```
Set-Cookie: {COOKIE_NAME}={session_id}; HttpOnly; Secure; Path=/; SameSite=None; Max-Age=86400
```

#### 응답
```json
{
  "account_info": {
    "account_id": "0x1234567890abcdef1234567890abcdef12345678",
    "nickname": "0x1234...5678",
    "bio": "",
    "image_uri": "https://storage.nadapp.net/default/1.png"
  }
}
```

#### 쿠키 설정

| 속성 | 값 | 설명 |
|------|------|------|
| `HttpOnly` | true | JavaScript 접근 불가 |
| `Secure` | true | HTTPS만 전송 |
| `Path` | `/` | 모든 경로에서 유효 |
| `SameSite` | None | Cross-site 요청에서도 전송 |
| `Max-Age` | 86400 | 24시간 유효 |

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 서명 형식, nonce 형식 등)
- `401`: 인증 실패 (서명 검증 실패, nonce 만료)
- `500`: 내부 서버 에러

---

### 3. 세션 삭제 (`DELETE /auth/delete_session`)

현재 세션을 삭제하고 로그아웃 처리합니다.

#### 요청
- **Method**: `DELETE`
- **인증**: 필수 (세션 쿠키)

#### 요청 헤더
```
Cookie: {COOKIE_NAME}={session_id}
```

#### 처리 과정

1. 세션 쿠키에서 세션 ID 추출
2. Redis에서 세션 삭제
3. 만료된 쿠키 설정으로 클라이언트 쿠키 제거

#### 응답 헤더
```
Set-Cookie: {COOKIE_NAME}=; HttpOnly; Secure; Path=/; SameSite=None; Max-Age=0
```

#### 응답
- **Status**: `200 OK`
- **Body**: 없음

#### 에러 응답
- `400`: 잘못된 요청 (세션 쿠키 누락)
- `500`: 내부 서버 에러

---

## TypeScript Interfaces

### 요청 타입

```typescript
// POST /auth/nonce
interface AuthNonceRequest {
  address: string;  // EVM 주소 (0x로 시작하는 40자리 hex)
}

// POST /auth/session
interface AuthSessionRequest {
  signature: string;        // ECDSA 서명 (0x + 130자리 hex)
  nonce: string;            // 서버에서 발급받은 nonce
  chain_id: number;         // 체인 ID (예: 10143)
  wallet_address?: string;  // 스마트 지갑 주소 (옵션)
}
```

### 응답 타입

```typescript
// POST /auth/nonce
interface AuthNonceResponse {
  nonce: string;  // 서명할 메시지
}

// POST /auth/session
interface AuthSessionResponse {
  account_info: AccountInfo;
}

interface AccountInfo {
  account_id: string;   // 지갑 주소
  nickname: string;     // 닉네임 (기본값: 주소)
  bio: string;          // 자기소개 (기본값: 빈 문자열)
  image_uri: string;    // 프로필 이미지 URL
}
```

---

## 인증 플로우

```
┌─────────────┐         ┌─────────────┐         ┌─────────────┐
│   Client    │         │   Server    │         │    Redis    │
└──────┬──────┘         └──────┬──────┘         └──────┬──────┘
       │                       │                       │
       │  POST /auth/nonce     │                       │
       │  { address }          │                       │
       │──────────────────────>│                       │
       │                       │   Store nonce         │
       │                       │──────────────────────>│
       │   { nonce }           │                       │
       │<──────────────────────│                       │
       │                       │                       │
       │  Sign nonce with      │                       │
       │  wallet               │                       │
       │                       │                       │
       │  POST /auth/session   │                       │
       │  { signature, nonce,  │                       │
       │    chain_id }         │                       │
       │──────────────────────>│                       │
       │                       │   Verify nonce        │
       │                       │──────────────────────>│
       │                       │                       │
       │                       │   Verify signature    │
       │                       │                       │
       │                       │   Create session      │
       │                       │──────────────────────>│
       │   { account_info }    │                       │
       │   Set-Cookie: nadfun-v3-api │                       │
       │<──────────────────────│                       │
       │                       │                       │
       │  (Authenticated       │                       │
       │   requests with       │                       │
       │   session cookie)     │                       │
       │                       │                       │
       │  DELETE /auth/        │                       │
       │  delete_session       │                       │
       │──────────────────────>│                       │
       │                       │   Delete session      │
       │                       │──────────────────────>│
       │   200 OK              │                       │
       │   Set-Cookie: expired │                       │
       │<──────────────────────│                       │
       │                       │                       │
```

---

## 환경변수

```env
# 세션 쿠키 이름 (필수)
COOKIE_NAME=api-session
```
