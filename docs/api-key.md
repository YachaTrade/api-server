# API Key 문서

## 개요

외부 서비스에서 nad.fun API에 접근하기 위한 API Key 시스템입니다.

### 동작 방식

| 요청 출처 | API Key 필요 | Rate Limit |
|----------|-------------|------------|
| nad.fun, nadapp.net, *.nad.fun, *.symphony.io | X | 없음 |
| localhost:* | X | 없음 |
| 기타 외부 Origin / Origin 없음 | O | 60 req/min |

### 제외 경로

다음 경로는 API Key 검사에서 제외됩니다:
- `/health`
- `/`
- `/cms/*`
- `/dev-sw*`
- `/api-key/*` (API Key 관리 - 세션 인증 사용)
- `/auth/*` (인증 엔드포인트)

---

## API Key 사용법

### 요청 헤더

```
X-API-Key: nadfun_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
```

### cURL 예시

```bash
curl https://api.nad.fun/token/list \
  -H "X-API-Key: nadfun_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
```

### Rate Limit 응답 헤더

모든 API Key 요청에 다음 헤더가 포함됩니다:

| 헤더 | 값 | 설명 |
|------|-----|------|
| `X-RateLimit-Limit` | 60 | 분당 허용 요청 수 |
| `X-RateLimit-Window` | 1m | Rate limit 윈도우 |

### Rate Limit 초과 시

```json
HTTP/1.1 429 Too Many Requests
Retry-After: 1

{
  "error": "Rate limit exceeded",
  "retry_after": 1
}
```

---

## API Key 관리 (로그인 사용자)

> **인증 필수**: 모든 API Key 관리 API는 세션 쿠키가 필요합니다. (로그인 필요)

### 1. API Key 생성 (`POST /api-key`)

새로운 API Key를 생성합니다. `owner_address`는 세션에서 자동으로 설정됩니다.

#### 요청

```bash
curl -X POST https://api.nad.fun/api-key \
  -H "Content-Type: application/json" \
  -H "Cookie: session=<user_session>" \
  -d '{
    "name": "My Integration",
    "description": "External service integration",
    "expires_in_days": 365
  }'
```

#### Request Body

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `name` | string | O | API Key 이름 |
| `description` | string | X | 설명 |
| `expires_in_days` | number | X | 만료 기간 (일). null = 무제한 |

#### 응답

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "api_key": "nadfun_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
  "key_prefix": "nadfun_xxxxxxxx",
  "name": "My Integration"
}
```

> **중요**: `api_key`는 생성 시 **한 번만** 반환됩니다. 안전하게 보관하세요!

---

### 2. API Key 목록 조회 (`GET /api-key`)

자신의 API Key 목록을 조회합니다.

#### 요청

```bash
curl https://api.nad.fun/api-key \
  -H "Cookie: session=<user_session>"
```

#### 응답

```json
{
  "api_keys": [
    {
      "id": "550e8400-e29b-41d4-a716-446655440000",
      "key_prefix": "nadfun_xxxxxxxx",
      "name": "My Integration",
      "description": "External service integration",
      "owner_address": "0x1234...",
      "is_active": true,
      "created_at": "2025-02-02T10:00:00Z",
      "expires_at": "2026-02-02T10:00:00Z",
      "last_used_at": "2025-02-02T12:30:00Z",
      "request_count": 1523
    }
  ],
  "total": 1
}
```

#### 응답 필드

| 필드 | 타입 | 설명 |
|------|------|------|
| `id` | UUID | API Key 고유 ID |
| `key_prefix` | string | Key 앞 12자리 (식별용) |
| `name` | string | API Key 이름 |
| `description` | string | 설명 |
| `owner_address` | string | 소유자 지갑 주소 |
| `is_active` | boolean | 활성화 상태 |
| `created_at` | datetime | 생성 시간 |
| `expires_at` | datetime | 만료 시간 (null = 무제한) |
| `last_used_at` | datetime | 마지막 사용 시간 |
| `request_count` | number | 총 요청 수 |

---

### 3. API Key 비활성화 (`DELETE /api-key/:id`)

자신의 API Key를 비활성화(revoke)합니다.

#### 요청

```bash
curl -X DELETE https://api.nad.fun/api-key/550e8400-e29b-41d4-a716-446655440000 \
  -H "Cookie: session=<user_session>"
```

#### 응답

```json
{
  "success": true
}
```

---

## TypeScript Interfaces

```typescript
// API Key 생성 요청
interface CreateApiKeyRequest {
  name: string;
  description?: string;
  owner_address?: string;
  expires_in_days?: number;  // null = 무제한
}

// API Key 생성 응답 (api_key는 이때만 반환됨!)
interface CreateApiKeyResponse {
  id: string;
  api_key: string;  // 한 번만 반환!
  key_prefix: string;
  name: string;
}

// API Key 정보 (목록 조회용)
interface ApiKeyInfo {
  id: string;
  key_prefix: string;
  name: string;
  description?: string;
  owner_address?: string;
  is_active: boolean;
  created_at: string;
  expires_at?: string;
  last_used_at?: string;
  request_count: number;
}

// API Key 목록 응답
interface ApiKeyListResponse {
  api_keys: ApiKeyInfo[];
  total: number;
}
```

---

## 에러 코드

| 상태 코드 | 설명 |
|-----------|------|
| 200 | 성공 |
| 401 | 인증 실패 (유효하지 않은 API Key, Admin 권한 없음) |
| 404 | API Key를 찾을 수 없음 |
| 429 | Rate limit 초과 |
| 500 | 내부 서버 에러 |

---

## 보안 참고사항

1. **API Key 보관**: 생성 시 한 번만 반환되므로 안전하게 보관
2. **환경 변수**: 코드에 하드코딩하지 말고 환경 변수로 관리
3. **만료 설정**: 가능하면 만료 기간을 설정하여 주기적으로 갱신
4. **비활성화**: 유출 시 즉시 비활성화 처리

```bash
# 환경 변수 예시
export NAD_API_KEY="nadfun_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
```

```javascript
// JavaScript 사용 예시
const apiKey = process.env.NAD_API_KEY;

fetch('https://api.nad.fun/token/list', {
  headers: {
    'X-API-Key': apiKey
  }
});
```
