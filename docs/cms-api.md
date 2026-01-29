# CMS API 문서

## 개요

CMS(Content Management System) API는 관리자 전용 토큰 관리 기능을 제공합니다.

- **NSFW 설정**: 토큰의 NSFW(Not Safe For Work) 상태 설정
- **트렌드 관리**: 트렌드 토큰 목록 관리
- **메타데이터 수정**: 토큰 메타데이터 업데이트 (설명, 링크, 이미지)
- **해커톤 등록**: 해커톤 프로젝트 등록 (별도 문서 참조: [hackathon-api.md](./hackathon-api.md))

> **인증 필수**: 모든 CMS API는 Admin 권한이 필요합니다.

---

## API 엔드포인트

### 1. 토큰 NSFW 설정 (`POST /cms/token/nsfw`)

토큰의 NSFW 상태를 설정합니다. **Admin 권한 필요**

#### 요청
- **Method**: `POST`
- **Content-Type**: `application/json`
- **인증**: 필수 (Admin only)

#### Request Body
```json
{
  "token_id": "0x1234567890abcdef...",
  "is_nsfw": true
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `token_id` | string | O | 토큰 컨트랙트 주소 (EVM 형식) |
| `is_nsfw` | boolean | O | NSFW 상태 (true: NSFW, false: SFW) |

#### 응답
```json
{
  "success": true
}
```

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 token_id)
- `401`: 인증 실패 (Admin 권한 없음)
- `500`: 내부 서버 에러

---

### 2. 트렌드 토큰 등록 (`POST /cms/trend/insert`)

트렌드 토큰 목록을 등록합니다. **Admin 권한 필요**

#### 요청
- **Method**: `POST`
- **Content-Type**: `application/json`
- **인증**: 필수 (Admin only)

#### Request Body
```json
{
  "token_ids": [
    "0x1234567890abcdef...",
    "0x5678901234abcdef...",
    "0xabcdef1234567890..."
  ]
}
```

#### 필드 설명

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `token_ids` | string[] | O | 토큰 ID 배열 (최대 50개) |

#### 유효성 검사
- 최대 50개의 토큰까지 등록 가능
- 각 토큰 ID는 유효한 EVM 주소 형식이어야 함

#### 응답
```json
{
  "success": true
}
```

#### 에러 응답
- `400`: 잘못된 요청 (토큰 개수 초과, 유효하지 않은 token_id)
- `401`: 인증 실패 (Admin 권한 없음)
- `500`: 내부 서버 에러

---

### 3. 토큰 메타데이터 수정 (`POST /cms/token/metadata`)

토큰의 메타데이터를 수정합니다. **Admin 권한 필요**

#### 요청
- **Method**: `POST`
- **Content-Type**: `multipart/form-data`
- **인증**: 필수 (Admin only)

#### Form Fields

| 필드 | 타입 | 필수 | 설명 |
|------|------|------|------|
| `token_id` | string | O | 토큰 컨트랙트 주소 (EVM 형식) |
| `description` | string | X | 토큰 설명 (최대 500자) |
| `website` | string | X | 웹사이트 URL (`https://`로 시작) |
| `twitter` | string | X | Twitter/X URL (`https://x.com/`으로 시작) |
| `telegram` | string | X | 텔레그램 URL (`https://t.me/`으로 시작) |
| `image` | file | X | 토큰 이미지 파일 |

#### 유효성 검사
- `token_id`: 유효한 EVM 주소 형식
- `description`: 최대 500자
- `website`: `https://`로 시작해야 함
- `twitter`: `https://x.com/`으로 시작해야 함
- `telegram`: `https://t.me/`으로 시작해야 함

#### 요청 예시 (cURL)
```bash
curl -X POST "https://api.example.com/cms/token/metadata" \
  -H "Authorization: Bearer <admin_token>" \
  -F "token_id=0x1234567890abcdef..." \
  -F "description=Updated token description" \
  -F "website=https://myproject.com" \
  -F "twitter=https://x.com/myproject" \
  -F "telegram=https://t.me/myproject" \
  -F "image=@/path/to/image.png"
```

#### 응답
```json
{
  "success": true,
  "metadata_uri": "https://storage.nadapp.net/metadata/0x1234567890abcdef.json"
}
```

#### 응답 필드

| 필드 | 타입 | 설명 |
|------|------|------|
| `success` | boolean | 성공 여부 |
| `metadata_uri` | string | 업데이트된 메타데이터 URI |

#### 에러 응답
- `400`: 잘못된 요청 (유효하지 않은 token_id, URL 형식 오류, 설명 길이 초과)
- `401`: 인증 실패 (Admin 권한 없음)
- `500`: 내부 서버 에러

---

### 4. 해커톤 프로젝트 등록 (`POST /cms/hackathon/register`)

해커톤 프로젝트를 등록합니다. 자세한 내용은 [hackathon-api.md](./hackathon-api.md) 문서를 참조하세요.

---

## TypeScript Interfaces

### 요청 타입

```typescript
// POST /cms/token/nsfw
interface SetNsfwRequest {
  token_id: string;  // EVM 주소 형식
  is_nsfw: boolean;
}

// POST /cms/trend/insert
interface InsertTrendRequest {
  token_ids: string[];  // 최대 50개, EVM 주소 형식
}

// POST /cms/token/metadata (multipart/form-data)
interface UpdateMetadataRequest {
  token_id: string;        // 필수, EVM 주소 형식
  description?: string;    // 최대 500자
  website?: string;        // https://로 시작
  twitter?: string;        // https://x.com/으로 시작
  telegram?: string;       // https://t.me/으로 시작
  image?: File;            // 이미지 파일
}
```

### 응답 타입

```typescript
// POST /cms/token/nsfw, POST /cms/trend/insert
interface CmsActionResponse {
  success: boolean;
}

// POST /cms/token/metadata
interface UpdateMetadataResponse {
  success: boolean;
  metadata_uri: string;
}
```

---

## 에러 코드 요약

| 상태 코드 | 설명 |
|-----------|------|
| 200 | 성공 |
| 400 | 잘못된 요청 (유효성 검사 실패) |
| 401 | 인증 실패 (Admin 권한 없음) |
| 500 | 내부 서버 에러 |

---

## 유효성 검사 규칙 요약

### token_id
- 유효한 EVM 주소 형식 (0x로 시작하는 40자리 16진수)

### URL 필드
- `website`: `https://`로 시작
- `twitter`: `https://x.com/`으로 시작
- `telegram`: `https://t.me/`으로 시작

### 제한사항
- `description`: 최대 500자
- `token_ids` (트렌드): 최대 50개
