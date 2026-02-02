# Work Plan: API Key 기반 속도 제한 구현 (v2 - Critic 피드백 반영)

## Context

### Original Request
Cloudflare -> HAProxy -> API Server (Rust/Axum) 인프라에서 API Key 기반 속도 제한 구현

### User Requirements
1. **`nad.fun` Origin 요청**: API Key 없이 기존 CORS/세션 인증으로 처리
2. **외부 요청 (nad.fun 이외)**: API Key 필수 + 초당 1회 제한
3. **Tier 없음**: 단일 Rate Limit 정책 (1 req/sec)

### Current State (Verified)
- **Framework**: Rust + Axum 0.7.5
- **Rate Limiting**: `tower_governor 0.4.2` (미사용)
- **Current Auth**: 세션 쿠키 기반 + Origin 검증
- **Allowed Origins**: `nad.fun`, `nadapp.net`, `*.nad.fun`, `*.symphony.io`, `localhost:*`
- **Migration naming**: `0001_`, `0002_`, ... `0014_hackathon.sql` (현재 마지막)
- **Services structure**: Flat files (`src/services/cms.rs`) 또는 directories (`src/services/trading/`)
- **AppError**: `Unauthorized(String)` 이미 존재, `TooManyRequests` 없음

---

## Architecture Decision

### 인증 흐름

```
                    ┌─────────────────────────────────────────┐
                    │              Request                    │
                    └─────────────────┬───────────────────────┘
                                      │
                                      ▼
                    ┌─────────────────────────────────────────┐
                    │       Origin Header Check               │
                    └─────────────────┬───────────────────────┘
                                      │
          ┌───────────────────────────┼───────────────────────────┐
          │                           │                           │
          ▼                           ▼                           ▼
┌─────────────────┐       ┌─────────────────┐       ┌─────────────────┐
│ nad.fun Origin  │       │  No Origin      │       │ External Origin │
│ (Allowed list)  │       │  Header         │       │ (Not in list)   │
└────────┬────────┘       └────────┬────────┘       └────────┬────────┘
         │                         │                         │
         ▼                         ▼                         ▼
┌─────────────────┐       ┌─────────────────┐       ┌─────────────────┐
│ 기존 CORS/세션  │       │ X-API-Key 필수  │       │ X-API-Key 필수  │
│ 인증 (No Rate)  │       │ + Rate Limit    │       │ + Rate Limit    │
└─────────────────┘       │ (1 req/sec)     │       │ (1 req/sec)     │
                          └─────────────────┘       └─────────────────┘
```

### Origin 없는 요청 처리 (Critical Decision)
**결정**: Origin 헤더가 없는 요청 = **API Key 필수**
- Server-to-server 호출, curl 등 Origin 없는 요청은 외부로 간주
- 이유: Origin 스푸핑 방지, 명시적 API Key 요구로 보안 강화

### Rate Limit 정책

| 구분 | Rate Limit | 적용 대상 |
|------|------------|-----------|
| nad.fun Origin (허용 목록) | **없음** | 기존 세션 인증 |
| 외부/Origin 없음 + API Key | **1 req/sec** | 모든 엔드포인트 |

### Middleware Architecture (Critical Decision)
**결정**: **단일 전역 미들웨어** + 경로 기반 제외
- 현재 `main.rs`는 전역 레이어 방식 사용
- `api_key_gate` 미들웨어를 전역으로 적용
- `/health`, `/cms/*` 경로는 미들웨어 내에서 제외

```rust
// main.rs - 전역 미들웨어로 적용
.layer(axum_middleware::from_fn_with_state(app_state.clone(), api_key_gate))
```

---

## Work Objectives

### Definition of Done
- [ ] `nad.fun` Origin 요청은 기존처럼 API Key 없이 동작
- [ ] Origin 없는 요청은 API Key 필수
- [ ] 외부 Origin 요청은 `X-API-Key` 헤더 필수
- [ ] 외부 요청은 초당 1회로 제한 (429 응답 + Retry-After 헤더)
- [ ] API Key CRUD 관리 (CMS - Admin 인증 필수)

---

## Detailed TODOs

### Phase 0: Dependencies

#### TODO 0.1: Cargo.toml에 sha2 크레이트 추가
**File:** `Cargo.toml` (수정)

```toml
# Utilities 섹션에 추가:
sha2 = "0.10"  # SHA256 hashing for API keys
```

**Note:** `rand = "0.8.5"`는 이미 존재함.

---

### Phase 1: Database & Types

#### TODO 1.1: API Key 테이블 마이그레이션
**File:** `migrations/0015_api_keys.sql` (신규)

```sql
-- API Keys table for external API access
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key_hash VARCHAR(64) NOT NULL UNIQUE,  -- SHA256 hash of API key
    key_prefix VARCHAR(12) NOT NULL,        -- First 12 chars (nad_xxxxxxxx)
    name VARCHAR(255) NOT NULL,
    description TEXT,
    owner_address VARCHAR(42),              -- Optional: link to account
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE,    -- NULL = never expires
    last_used_at TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN DEFAULT TRUE,
    request_count BIGINT DEFAULT 0          -- Total request count for analytics
);

-- Index for fast key lookup
CREATE INDEX idx_api_keys_key_hash ON api_keys(key_hash);
-- Index for active keys only
CREATE INDEX idx_api_keys_active ON api_keys(is_active) WHERE is_active = TRUE;
```

**Acceptance Criteria:**
- [ ] 파일명: `0015_api_keys.sql` (기존 네이밍 규칙 준수)
- [ ] `sqlx migrate run` 성공

---

#### TODO 1.2: API Key 타입 정의
**File:** `src/types/api_key.rs` (신규)

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;
use uuid::Uuid;

/// Database model for API keys
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ApiKey {
    pub id: Uuid,
    pub key_hash: String,
    pub key_prefix: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub request_count: i64,
}

impl ApiKey {
    /// Check if the API key is valid (active and not expired)
    pub fn is_valid(&self) -> bool {
        if !self.is_active {
            return false;
        }
        if let Some(expires_at) = self.expires_at {
            return Utc::now() < expires_at;
        }
        true
    }
}

/// Cached API key info stored in Redis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedApiKey {
    pub id: Uuid,
    pub key_hash: String,
    pub is_active: bool,
    pub expires_at: Option<DateTime<Utc>>,
}

/// Request to create a new API key
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateApiKeyRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub owner_address: Option<String>,
    /// Expiration in days (None = never expires)
    #[serde(default)]
    pub expires_in_days: Option<i64>,
}

/// Response when creating a new API key
/// NOTE: api_key is only returned once at creation time
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateApiKeyResponse {
    pub id: Uuid,
    /// Full API key - ONLY returned at creation time, store securely!
    pub api_key: String,
    pub key_prefix: String,
    pub name: String,
}

/// API key info for listing (without sensitive data)
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyInfo {
    pub id: Uuid,
    pub key_prefix: String,
    pub name: String,
    pub description: Option<String>,
    pub owner_address: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub request_count: i64,
}

impl From<ApiKey> for ApiKeyInfo {
    fn from(key: ApiKey) -> Self {
        Self {
            id: key.id,
            key_prefix: key.key_prefix,
            name: key.name,
            description: key.description,
            owner_address: key.owner_address,
            is_active: key.is_active,
            created_at: key.created_at,
            expires_at: key.expires_at,
            last_used_at: key.last_used_at,
            request_count: key.request_count,
        }
    }
}

/// API key list response
#[derive(Debug, Serialize, ToSchema)]
pub struct ApiKeyListResponse {
    pub api_keys: Vec<ApiKeyInfo>,
    pub total: i64,
}
```

**File:** `src/types/mod.rs` (수정 - 한 줄 추가)
```rust
pub mod api_key;  // 추가
```

**Acceptance Criteria:**
- [ ] `src/types/api_key.rs` 생성
- [ ] `src/types/mod.rs`에 모듈 추가
- [ ] 컴파일 성공

---

### Phase 2: Core Services

#### TODO 2.1: API Key 서비스
**File:** `src/services/api_key.rs` (신규 - flat file 패턴)

```rust
use crate::db::postgres::PostgresDatabase;
use crate::db::redis::RedisDatabase;
use crate::result::AppError;
use crate::types::api_key::{ApiKey, CachedApiKey, CreateApiKeyRequest, CreateApiKeyResponse};
use chrono::{Duration, Utc};
use rand::Rng;
use sha2::{Digest, Sha256};
use uuid::Uuid;

const API_KEY_PREFIX: &str = "nad_";
const API_KEY_LENGTH: usize = 32;
const API_KEY_CACHE_TTL_SECS: u64 = 300; // 5 minutes

/// Generate a new API key with prefix
pub fn generate_api_key() -> String {
    let random_part: String = rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(API_KEY_LENGTH)
        .map(char::from)
        .collect();
    format!("{}{}", API_KEY_PREFIX, random_part)
}

/// Hash an API key using SHA256
pub fn hash_api_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Create a new API key
pub async fn create_api_key(
    db: &PostgresDatabase,
    req: CreateApiKeyRequest,
) -> Result<CreateApiKeyResponse, AppError> {
    let api_key = generate_api_key();
    let key_hash = hash_api_key(&api_key);
    let key_prefix = api_key[..12].to_string(); // "nad_" + 8 chars

    let expires_at = req
        .expires_in_days
        .map(|days| Utc::now() + Duration::days(days));

    let id = sqlx::query_scalar!(
        r#"
        INSERT INTO api_keys (key_hash, key_prefix, name, description, owner_address, expires_at)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id
        "#,
        key_hash,
        key_prefix,
        req.name,
        req.description,
        req.owner_address,
        expires_at,
    )
    .fetch_one(db.write_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Failed to create API key: {}", e)))?;

    Ok(CreateApiKeyResponse {
        id,
        api_key, // Only time this is returned!
        key_prefix,
        name: req.name,
    })
}

/// Validate an API key and return its info
pub async fn validate_api_key(
    db: &PostgresDatabase,
    redis: &RedisDatabase,
    api_key: &str,
) -> Result<CachedApiKey, AppError> {
    // Validate format
    if !api_key.starts_with(API_KEY_PREFIX) || api_key.len() != API_KEY_PREFIX.len() + API_KEY_LENGTH {
        return Err(AppError::Unauthorized("Invalid API key format".to_string()));
    }

    let key_hash = hash_api_key(api_key);

    // Check Redis cache first
    if let Ok(Some(cached)) = redis.get_cached_api_key(&key_hash).await {
        if let Ok(info) = serde_json::from_str::<CachedApiKey>(&cached) {
            if info.is_active {
                if let Some(expires_at) = info.expires_at {
                    if Utc::now() >= expires_at {
                        return Err(AppError::Unauthorized("API key expired".to_string()));
                    }
                }
                return Ok(info);
            }
            return Err(AppError::Unauthorized("API key is inactive".to_string()));
        }
    }

    // Cache miss - query database
    let api_key_record = sqlx::query_as!(
        ApiKey,
        r#"
        SELECT id, key_hash, key_prefix, name, description, owner_address,
               created_at, expires_at, last_used_at, is_active, request_count
        FROM api_keys
        WHERE key_hash = $1
        "#,
        key_hash,
    )
    .fetch_optional(db.read_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?
    .ok_or_else(|| AppError::Unauthorized("API key not found".to_string()))?;

    if !api_key_record.is_valid() {
        return Err(AppError::Unauthorized("API key is invalid or expired".to_string()));
    }

    // Cache the result
    let cached = CachedApiKey {
        id: api_key_record.id,
        key_hash: api_key_record.key_hash.clone(),
        is_active: api_key_record.is_active,
        expires_at: api_key_record.expires_at,
    };

    let _ = redis
        .cache_api_key(&key_hash, &serde_json::to_string(&cached).unwrap(), API_KEY_CACHE_TTL_SECS)
        .await;

    Ok(cached)
}

/// Update last_used_at timestamp (fire and forget)
pub async fn update_last_used(db: &PostgresDatabase, key_hash: &str) {
    let _ = sqlx::query!(
        "UPDATE api_keys SET last_used_at = NOW(), request_count = request_count + 1 WHERE key_hash = $1",
        key_hash
    )
    .execute(db.write_pool())
    .await;
}

/// List all API keys
pub async fn list_api_keys(db: &PostgresDatabase) -> Result<Vec<ApiKey>, AppError> {
    sqlx::query_as!(
        ApiKey,
        r#"
        SELECT id, key_hash, key_prefix, name, description, owner_address,
               created_at, expires_at, last_used_at, is_active, request_count
        FROM api_keys
        ORDER BY created_at DESC
        "#,
    )
    .fetch_all(db.read_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))
}

/// Revoke (deactivate) an API key
pub async fn revoke_api_key(db: &PostgresDatabase, redis: &RedisDatabase, id: Uuid) -> Result<(), AppError> {
    let result = sqlx::query!(
        "UPDATE api_keys SET is_active = FALSE WHERE id = $1 RETURNING key_hash",
        id
    )
    .fetch_optional(db.write_pool())
    .await
    .map_err(|e| AppError::InternalError(format!("Database error: {}", e)))?;

    if let Some(record) = result {
        // Invalidate cache
        let _ = redis.delete_cached_api_key(&record.key_hash).await;
        Ok(())
    } else {
        Err(AppError::NotFound("API key not found".to_string()))
    }
}
```

**File:** `src/services/mod.rs` (수정 - 두 줄 추가)
```rust
pub mod api_key;      // 추가
pub mod rate_limiter; // 추가
```

**Acceptance Criteria:**
- [ ] `nad_` prefix + 32자 랜덤 키 생성
- [ ] SHA256 해시 저장
- [ ] Redis 캐시 연동 (5분 TTL)
- [ ] 만료/비활성 키 거부

---

#### TODO 2.2: Rate Limiter 서비스
**File:** `src/services/rate_limiter.rs` (신규)

```rust
use crate::db::redis::RedisDatabase;
use crate::result::AppError;
use std::time::{SystemTime, UNIX_EPOCH};

/// Rate limit check result
#[derive(Debug)]
pub enum RateLimitResult {
    Allowed,
    Exceeded { retry_after: u64 },
}

/// Check rate limit and increment counter (1 request per second)
/// Redis key format: rate:{key_hash}:sec:{unix_second}
pub async fn check_and_increment(
    redis: &RedisDatabase,
    key_hash: &str,
) -> Result<RateLimitResult, AppError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let redis_key = format!("rate:{}:sec:{}", key_hash, now);

    // Atomic INCR + EXPIRE
    let count = redis
        .incr_with_expire(&redis_key, 2) // TTL 2 seconds for safety
        .await
        .map_err(|e| AppError::InternalError(format!("Rate limit check failed: {}", e)))?;

    if count > 1 {
        return Ok(RateLimitResult::Exceeded { retry_after: 1 });
    }

    Ok(RateLimitResult::Allowed)
}
```

---

### Phase 3: Redis Methods

#### TODO 3.1: Redis 메서드 추가
**File:** `src/db/redis/mod.rs` (수정 - 메서드 추가)

```rust
// 새로운 impl 블록으로 API Key 관련 메서드 추가 (파일 하단에):

// API Key Rate Limiting
impl RedisDatabase {
    /// Atomic INCR with EXPIRE (for rate limiting)
    /// Returns the current count after increment
    pub async fn incr_with_expire(&self, key: &str, ttl_secs: u64) -> Result<u64> {
        let mut conn = self.conn.as_ref().clone();

        // Use INCR command
        let count: u64 = redis::cmd("INCR")
            .arg(key)
            .query_async(&mut conn)
            .await?;

        // Set TTL only on first increment (when count == 1)
        if count == 1 {
            let _: () = redis::cmd("EXPIRE")
                .arg(key)
                .arg(ttl_secs)
                .query_async(&mut conn)
                .await?;
        }

        Ok(count)
    }

    /// Cache API key info (JSON string)
    pub async fn cache_api_key(&self, key_hash: &str, info: &str, ttl_secs: u64) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let redis_key = format!("api_key:{}", key_hash);

        let _: () = redis::cmd("SETEX")
            .arg(&redis_key)
            .arg(ttl_secs)
            .arg(info)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    /// Get cached API key info
    pub async fn get_cached_api_key(&self, key_hash: &str) -> Result<Option<String>> {
        let mut conn = self.conn.as_ref().clone();
        let redis_key = format!("api_key:{}", key_hash);

        let result: Option<String> = redis::cmd("GET")
            .arg(&redis_key)
            .query_async(&mut conn)
            .await?;

        Ok(result)
    }

    /// Delete cached API key (on revocation)
    pub async fn delete_cached_api_key(&self, key_hash: &str) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let redis_key = format!("api_key:{}", key_hash);

        let _: () = redis::cmd("DEL")
            .arg(&redis_key)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }
}
```

**Acceptance Criteria:**
- [ ] `incr_with_expire` 구현
- [ ] `cache_api_key`, `get_cached_api_key`, `delete_cached_api_key` 구현

---

### Phase 4: Middleware & Error

#### TODO 4.1: AppError에 TooManyRequests 추가
**File:** `src/result.rs` (수정)

```rust
// AppError enum에 추가 (Unauthorized(String) 이미 존재함):
pub enum AppError {
    // ... 기존 variants ...
    TooManyRequests { retry_after: u64 },  // 추가
}

// IntoResponse impl에 매칭 추가:
impl IntoResponse for AppError {
    fn into_response(self) -> Response<axum::body::Body> {
        let (status, error_message) = match self {
            // ... 기존 매칭들 ...
            AppError::TooManyRequests { retry_after } => {
                // 429 응답에 Retry-After 헤더 포함
                let body = Json(json!({
                    "error": "Rate limit exceeded",
                    "retry_after": retry_after
                }));
                let mut response = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
                response.headers_mut().insert(
                    "Retry-After",
                    retry_after.to_string().parse().unwrap(),
                );
                return response;
            }
        };
        // ... 기존 코드 ...
    }
}
```

**Acceptance Criteria:**
- [ ] `TooManyRequests { retry_after: u64 }` variant 추가
- [ ] 429 응답 + `Retry-After` 헤더

---

#### TODO 4.2: API Key Gate 미들웨어
**File:** `src/middleware.rs` (수정)

```rust
// 파일 상단에 import 추가:
use crate::services::api_key::{validate_api_key, update_last_used};
use crate::services::rate_limiter::{check_and_increment, RateLimitResult};
use axum::http::HeaderMap;

/// API Key 검증 및 Rate Limit 미들웨어 (전역 적용)
/// - nad.fun 등 허용된 Origin: 통과 (기존 동작)
/// - Origin 없음 또는 외부 Origin: X-API-Key 필수 + 1 req/sec
pub async fn api_key_gate(
    State(state): State<AppState>,
    headers: HeaderMap,
    req: Request<Body>,
    next: Next,
) -> Result<Response<Body>, AppError> {
    // 0. 제외할 경로 확인
    let path = req.uri().path();
    if path == "/health" || path == "/" || path.starts_with("/cms/") || path.starts_with("/dev-sw") {
        return Ok(next.run(req).await);
    }

    // 1. Origin 헤더 확인
    let origin = headers
        .get(ORIGIN)
        .and_then(|v| v.to_str().ok());

    // 2. 허용된 Origin이면 API Key 검사 건너뛰기
    if let Some(origin_str) = origin {
        if is_allowed_origin(origin_str) {
            // nad.fun, nadapp.net 등 → 기존 플로우 (API Key 불필요)
            return Ok(next.run(req).await);
        }
    }

    // 3. 외부 Origin 또는 Origin 없음 → API Key 필수
    let api_key = headers
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("X-API-Key header required".to_string()))?;

    // 4. API Key 검증 (Redis 캐시 → DB)
    let key_info = validate_api_key(&state.postgres, &state.redis, api_key).await?;

    // 5. Rate Limit 확인 (1 req/sec)
    match check_and_increment(&state.redis, &key_info.key_hash).await? {
        RateLimitResult::Exceeded { retry_after } => {
            return Err(AppError::TooManyRequests { retry_after });
        }
        RateLimitResult::Allowed => {}
    }

    // 6. last_used_at 업데이트 (비동기, fire-and-forget)
    let db = state.postgres.clone();
    let hash = key_info.key_hash.clone();
    tokio::spawn(async move {
        update_last_used(&db, &hash).await;
    });

    // 7. 요청 처리
    let mut response = next.run(req).await;

    // 8. Rate Limit 헤더 추가
    response.headers_mut().insert(
        "X-RateLimit-Limit",
        "1".parse().unwrap(),
    );
    response.headers_mut().insert(
        "X-RateLimit-Window",
        "1s".parse().unwrap(),
    );

    Ok(response)
}
```

**Acceptance Criteria:**
- [ ] `/health`, `/`, `/cms/*`, `/dev-sw*` 경로 제외
- [ ] 허용된 Origin → 통과
- [ ] Origin 없음 → API Key 필수
- [ ] 외부 Origin → API Key 필수
- [ ] Rate Limit 초과 시 429 + Retry-After
- [ ] 응답에 Rate Limit 헤더 추가

---

### Phase 5: Router Integration

#### TODO 5.1: main.rs에 전역 미들웨어 적용
**File:** `src/main.rs` (수정)

```rust
// import 추가
use api_server::middleware::api_key_gate;

// 기존 레이어들 뒤에 api_key_gate 미들웨어 추가
let app = Router::new()
    .merge(root)
    .merge(health::router())
    // ... 모든 라우터 merge ...
    .layer(DefaultBodyLimit::max(100_000))
    .layer(axum_middleware::from_fn(method_based_timeout))
    .layer(axum_middleware::from_fn_with_state(app_state.clone(), api_key_gate))  // 추가
    .layer(ServiceBuilder::new().layer(get_cors()).into_inner())
    .layer(cookie_manager_layer)
    .with_state(app_state)
    .fallback(handler_404);
```

**Acceptance Criteria:**
- [ ] 전역 미들웨어로 `api_key_gate` 적용
- [ ] 기존 레이어 순서 유지

---

#### TODO 5.2: CMS API Key 관리 엔드포인트
**File:** `src/router/cms/path.rs` (수정 - enum에 variant 추가)

```rust
#[derive(Debug, Clone, Copy)]
pub enum CmsPath {
    SetNsfw,
    InsertTrend,
    UpdateMetadata,
    RegisterHackathon,
    ApiKey,      // 추가
    ApiKeyId,    // 추가
}

impl CmsPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            CmsPath::SetNsfw => "/cms/token/nsfw",
            CmsPath::InsertTrend => "/cms/trend/insert",
            CmsPath::UpdateMetadata => "/cms/token/metadata",
            CmsPath::RegisterHackathon => "/cms/hackathon/register",
            CmsPath::ApiKey => "/cms/api-key",        // 추가
            CmsPath::ApiKeyId => "/cms/api-key/:id",  // 추가
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            CmsPath::SetNsfw => "/cms/token/nsfw",
            CmsPath::InsertTrend => "/cms/trend/insert",
            CmsPath::UpdateMetadata => "/cms/token/metadata",
            CmsPath::RegisterHackathon => "/cms/hackathon/register",
            CmsPath::ApiKey => "/cms/api-key",        // 추가
            CmsPath::ApiKeyId => "/cms/api-key/{id}", // 추가 (OpenAPI format)
        }
    }
}
```

**File:** `src/router/cms/handler.rs` (수정 - 핸들러 추가)

```rust
// 파일 상단 imports에 추가:
use crate::services::api_key::{create_api_key, list_api_keys, revoke_api_key};
use crate::types::api_key::{CreateApiKeyRequest, CreateApiKeyResponse, ApiKeyInfo, ApiKeyListResponse};
use axum::routing::get;

/// Create new API key (Admin only)
#[utoipa::path(
    post,
    path = "/cms/api-key",
    request_body = CreateApiKeyRequest,
    responses(
        (status = 200, description = "API key created", body = CreateApiKeyResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "CMS"
)]
pub async fn create_api_key_handler(
    State(state): State<AppState>,
    Extension(admin_address): Extension<String>, // Admin 인증 확인
    Json(req): Json<CreateApiKeyRequest>,
) -> Result<Json<CreateApiKeyResponse>, AppError> {
    let response = create_api_key(&state.postgres, req).await?;
    Ok(Json(response))
}

/// List all API keys (Admin only)
#[utoipa::path(
    get,
    path = "/cms/api-key",
    responses(
        (status = 200, description = "API key list", body = ApiKeyListResponse),
        (status = 401, description = "Unauthorized"),
    ),
    tag = "CMS"
)]
pub async fn list_api_keys_handler(
    State(state): State<AppState>,
    Extension(_admin_address): Extension<String>,
) -> Result<Json<ApiKeyListResponse>, AppError> {
    let keys = list_api_keys(&state.postgres).await?;
    let total = keys.len() as i64;
    let api_keys: Vec<ApiKeyInfo> = keys.into_iter().map(ApiKeyInfo::from).collect();
    Ok(Json(ApiKeyListResponse { api_keys, total }))
}

/// Revoke API key (Admin only)
#[utoipa::path(
    delete,
    path = "/cms/api-key/{id}",
    params(
        ("id" = Uuid, Path, description = "API key ID")
    ),
    responses(
        (status = 200, description = "API key revoked"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "API key not found"),
    ),
    tag = "CMS"
)]
pub async fn revoke_api_key_handler(
    State(state): State<AppState>,
    Extension(_admin_address): Extension<String>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, AppError> {
    revoke_api_key(&state.postgres, &state.redis, id).await?;
    Ok(StatusCode::OK)
}
```

**File:** `src/router/cms/mod.rs` (수정 - 라우터에 추가)

```rust
use axum::routing::{post, get, delete};  // delete 추가

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        // ... 기존 라우트들 ...
        .route(
            CmsPath::ApiKey.as_str(),
            post(handler::create_api_key_handler)
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            CmsPath::ApiKey.as_str(),
            get(handler::list_api_keys_handler)
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
        .route(
            CmsPath::ApiKeyId.as_str(),
            delete(handler::revoke_api_key_handler)
                .layer(from_fn_with_state(state.clone(), authenticate_user)),
        )
}
```

**Acceptance Criteria:**
- [ ] POST /cms/api-key - 생성 (Admin 인증 필수)
- [ ] GET /cms/api-key - 목록 (Admin 인증 필수)
- [ ] DELETE /cms/api-key/:id - 비활성화 (Admin 인증 필수)
- [ ] CMS 라우터는 기존 `authenticate_user` 미들웨어로 Admin 검증

---

## File Changes Summary

| File | Action | Description |
|------|--------|-------------|
| `migrations/0015_api_keys.sql` | **Create** | API Key 테이블 |
| `src/types/api_key.rs` | **Create** | API Key 타입 |
| `src/types/mod.rs` | Modify | `pub mod api_key;` 추가 |
| `src/services/api_key.rs` | **Create** | API Key 서비스 |
| `src/services/rate_limiter.rs` | **Create** | Rate Limiter |
| `src/services/mod.rs` | Modify | 2개 모듈 추가 |
| `src/db/redis/mod.rs` | Modify | 4개 메서드 추가 |
| `src/result.rs` | Modify | `TooManyRequests` variant 추가 |
| `src/middleware.rs` | Modify | `api_key_gate` 함수 추가 |
| `src/main.rs` | Modify | 전역 미들웨어 레이어 추가 |
| `src/router/cms/path.rs` | Modify | 2개 경로 추가 |
| `src/router/cms/handler.rs` | Modify | 3개 핸들러 추가 |
| `src/router/cms/mod.rs` | Modify | 3개 라우트 추가 |

---

## Commit Strategy

```
1. feat(db): add api_keys table migration (0015)
2. feat(types): add API key types
3. feat(services): implement API key service with Redis cache
4. feat(services): implement rate limiter (1 req/sec)
5. feat(db): add Redis methods for rate limiting
6. feat(error): add TooManyRequests error variant
7. feat(middleware): add api_key_gate middleware
8. feat(router): add CMS API key management endpoints
9. feat(main): apply api_key_gate middleware globally
```

---

## Success Criteria

| Scenario | Expected Result |
|----------|-----------------|
| nad.fun Origin 요청 | API Key 없이 기존처럼 동작 |
| Origin 없는 요청 (API Key 없음) | 401 Unauthorized |
| 외부 Origin 요청 (API Key 없음) | 401 Unauthorized |
| 외부 요청 (API Key 있음, 첫 요청) | 200 OK + Rate Limit 헤더 |
| 외부 요청 (1초 내 두 번째 요청) | 429 Too Many Requests + Retry-After: 1 |
| /health 요청 | API Key 없이 200 OK |
| /cms/* 요청 | 기존 Admin 인증만 (API Key 불필요) |

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| Redis 장애 시 Rate Limit 우회 | 로깅 후 요청 허용 (graceful degradation) |
| Origin 헤더 스푸핑 | Cloudflare의 CF-Connecting-IP로 추가 검증 가능 |
| API Key 유출 | key_prefix만 표시, 전체 키는 생성 시 1회만 반환 |
| DB 부하 | Redis 캐시 (5분 TTL)로 DB 조회 최소화 |
