# Metadata API 문서

## 개요

Metadata API는 토큰 생성에 필요한 이미지와 메타데이터를 처리하는 API입니다. 두 단계로 구성되어 있습니다:

1. **이미지 업로드 및 NSFW 검증**
2. **메타데이터 업로드 및 저장**

## API 엔드포인트

### 1. 이미지 업로드 (`POST /metadata/image`)

토큰 이미지를 업로드하고 NSFW 여부를 검증합니다.

#### 요청
- **Method**: `POST`
- **Content-Type**: `multipart/form-data`
- **Body**: 
  - `image` (file): 이미지 파일 (JPEG, PNG, WebP, SVG)

#### 응답
```json
{
  "is_nsfw": false,
  "image_url": "https://storage.nadapp.net/coin/uuid-v4-string"
}
```

#### 처리 과정
1. 이미지 형식 검증 (MIME 타입 + Magic Bytes)
2. AWS Rekognition을 통한 NSFW 검증 (병렬 처리)
3. Cloudflare R2에 이미지 저장 (병렬 처리)
4. Redis에 NSFW 결과 캐싱 (3분 TTL)

#### 에러 응답
- `400`: 잘못된 이미지 형식 또는 이미지 없음
- `500`: NSFW 검증 실패 또는 업로드 실패

---

### 2. 메타데이터 업로드 (`POST /metadata/metadata`)

토큰 메타데이터를 업로드하고 R2 및 데이터베이스에 저장합니다.

#### 요청
- **Method**: `POST`
- **Content-Type**: `application/json`
- **Body**:
```json
{
  "image_url": "https://storage.nadapp.net/coin/uuid-v4-string",  // 필수: 이미지 업로드 API에서 반환된 URL
  "name": "Token Name",                                          // 필수: 토큰 이름
  "symbol": "SYMBOL",                                           // 필수: 토큰 심볼
  "description": "Token description",                           // 필수: 토큰 설명
  "website": "https://example.com",                            // 선택: 웹사이트 URL
  "twitter": "https://x.com/username",                         // 선택: X(트위터) URL
  "telegram": "https://t.me/username"                          // 선택: 텔레그램 URL
}
```

#### ⚠️ 주의사항

**필수 필드:**
- `image_url`: 반드시 이미지 업로드 API (`/metadata/image`)를 통해 먼저 업로드된 URL이어야 함
- `name`, `symbol`, `description`: 공백 또는 빈 문자열 불가

**URL 필드 규칙:**
- `website`: 반드시 `https://`로 시작해야 함 (예: `https://example.com`)
- `twitter`: 반드시 `x.com` 도메인을 포함하고 `https://`로 시작해야 함 (예: `https://x.com/username`)
- `telegram`: 반드시 `t.me` 도메인을 포함하고 `https://`로 시작해야 함 (예: `https://t.me/username`)
- 선택 필드는 빈 문자열(`""`) 또는 `null`로 설정 가능

**🔒 보안 정책:**
- 모든 URL은 보안을 위해 HTTPS만 허용
- HTTP로 시작하는 URL은 거부됨

**이미지 URL 제한:**
- `image_url`은 반드시 환경 변수 `ALLOWED_IMAGE_DOMAIN`에 설정된 도메인으로 시작해야 함
- 기본값: `https://storage.nadapp.net/`

**API 호출 순서:**
1. 먼저 `/metadata/image`로 이미지 업로드
2. 반환받은 `image_url`을 사용하여 `/metadata/metadata` 호출
3. 이미지 업로드 후 3분 이내에 메타데이터 업로드 완료 (Redis 캐시 만료 시간)

#### 응답
```json
{
  "metadata_url": "https://storage.nadapp.net/metadata/uuid-v4-string",
  "metadata": {
    "name": "Token Name",
    "symbol": "SYMBOL",
    "description": "Token description",
    "image_url": "https://storage.nadapp.net/coin/uuid-v4-string",
    "website": "https://example.com",
    "twitter": "https://x.com/username",
    "telegram": "https://t.me/username",
    "is_nsfw": false
  }
}
```

#### 처리 과정
1. Redis에서 이미지 NSFW 상태 확인 (필수)
2. 메타데이터 유효성 검증
3. R2에 JSON 형태로 메타데이터 저장
4. PostgreSQL 데이터베이스에 메타데이터 저장

#### 에러 응답
- `400`: NSFW 상태 불명 또는 유효하지 않은 데이터
- `500`: R2 또는 데이터베이스 저장 실패

---

## 데이터 검증 규칙

### 필수 필드
- `name`: 공백이 아닌 문자열
- `symbol`: 공백이 아닌 문자열
- `description`: 공백이 아닌 문자열
- `image_url`: 공백이 아니며 `ALLOWED_IMAGE_DOMAIN`으로 시작

### URL 검증 규칙
- **Website**: `https://`로 시작 (보안상 HTTPS만 허용)
- **Twitter**: `x.com` 포함 + `https://`로 시작 (보안상 HTTPS만 허용)
- **Telegram**: `t.me` 포함 + `https://`로 시작 (보안상 HTTPS만 허용)

### 이미지 검증 규칙
- **허용 형식**: JPEG, PNG, WebP, SVG
- **MIME 타입**: `image/jpeg`, `image/png`, `image/webp`, `image/svg+xml`
- **Magic Bytes 검증**: 실제 파일 형식과 선언된 MIME 타입 일치 확인

---

## 환경 변수

```env
# 허용된 이미지 도메인
ALLOWED_IMAGE_DOMAIN=https://storage.nadapp.net/

# NSFW 캐시 만료 시간 (초)
NSFW_STATUS_EXPIRATION=180

# AWS Rekognition 설정
AWS_REGION=ap-northeast-1

# R2 설정
R2_BUCKET_NAME=nadfuntestnet
R2_ACCESS_KEY=your-access-key
R2_SECRET_ACCESS_KEY=your-secret-key
CLOUDFLARE_ACCOUNT_ID=your-account-id
```

---

## 사용 플로우

### 일반적인 사용 시나리오

1. **이미지 업로드**
   ```bash
   curl -X POST http://localhost:8000/metadata/image \
     -F "image=@token-image.png"
   ```

2. **메타데이터 업로드**
   ```bash
   curl -X POST http://localhost:8000/metadata/metadata \
     -H "Content-Type: application/json" \
     -d '{
       "image_url": "https://storage.nadapp.net/coin/12345",
       "name": "My Token",
       "symbol": "MTK",
       "description": "This is my awesome token",
       "website": "https://mytoken.com",
       "twitter": "https://x.com/mytoken",
       "telegram": "https://t.me/mytoken"
     }'
   ```

---

## 성능 최적화

### 병렬 처리
- 이미지 업로드와 NSFW 검증을 `tokio::join!`으로 병렬 실행
- R2 업로드와 데이터베이스 저장 순차 실행 (의존성)

### 캐싱
- NSFW 검증 결과를 Redis에 3분간 캐싱
- 동일한 이미지 재업로드 시 NSFW 검증 스킵

### 로깅
- 각 단계별 실행 시간 측정 및 로깅
- 100ms 이상 소요되는 데이터베이스 쿼리 경고 로그

---

## 데이터베이스 스키마

```sql
CREATE TABLE token_metadata (
    metadata_url VARCHAR NOT NULL PRIMARY KEY,
    name VARCHAR NOT NULL,
    symbol VARCHAR NOT NULL,
    description TEXT NOT NULL,
    image_url VARCHAR,
    website VARCHAR,
    twitter VARCHAR,
    telegram VARCHAR,
    is_nsfw BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX idx_token_metadata_name ON token_metadata (name);
CREATE INDEX idx_token_metadata_symbol ON token_metadata (symbol);
CREATE INDEX idx_token_metadata_is_nsfw ON token_metadata (is_nsfw);
```

---

## 보안 고려사항

1. **이미지 검증**: Magic Bytes를 통한 실제 파일 형식 확인
2. **도메인 제한**: 허용된 도메인의 이미지만 허용
3. **NSFW 검증**: AWS Rekognition을 통한 부적절한 콘텐츠 차단
4. **URL 검증**: 소셜 미디어 플랫폼별 도메인 검증
5. **입력 검증**: 모든 필수 필드의 공백 문자열 방지