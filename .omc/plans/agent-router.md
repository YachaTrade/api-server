# Implementation Plan: Agent Router for nad.fun API Server

## Overview

Create a new `/agent` router in the Rust/Axum API server that provides a unified API for AI agents and third-party integrations. The router will expose endpoints for trading data (chart, swap_history, market), token data (token information, metadata), user holdings, and token creation with rewards. All endpoints will require API Key authentication via the existing `X-API-Key` header mechanism.

## Requirements

1. **Trading Data Endpoints**
   - GET `/agent/chart/:token_id` - Price chart data (OHLCV)
   - GET `/agent/swap-history/:token_id` - Swap/trade history for a token
   - GET `/agent/market/:token_id` - Current market data (price, volume, liquidity)
   - GET `/agent/metrics/:token_id` - Trading metrics

2. **Token Data Endpoints**
   - GET `/agent/token/:token_id` - Full token information with creator
   - GET `/agent/token/metadata/:token_id` - Token metadata (name, symbol, image, socials)

3. **Holding Token Endpoints**
   - GET `/agent/holdings/:account_id` - User's token holdings with balances

4. **Create Token Endpoints**
   - POST `/agent/token/image` - Upload token image
   - POST `/agent/token/metadata` - Upload token metadata
   - GET `/agent/token/created/:account_id` - Tokens created by account with rewards

## Acceptance Criteria

- [ ] All endpoints require valid `X-API-Key` header (enforced by existing middleware)
- [ ] All endpoints return consistent JSON response format
- [ ] All endpoints have OpenAPI documentation via utoipa
- [ ] All endpoints validate input parameters
- [ ] All endpoints use existing services (no database logic duplication)
- [ ] Rate limiting applies to all agent endpoints (60 req/min via existing middleware)
- [ ] Error responses follow existing `AppError` patterns
- [ ] Build passes with no warnings

## Implementation Steps

### Phase 1: Router Structure Setup

#### Step 1.1: Create Path Definitions
**File:** `src/router/agent/path.rs`

```rust
#[derive(Debug, Clone, Copy)]
pub enum AgentPath {
    GetChart,
    GetSwapHistory,
    GetMarket,
    GetMetrics,
    GetToken,
    GetTokenMetadata,
    GetHoldings,
    UploadImage,
    UploadMetadata,
    GetTokensCreated,
}

impl AgentPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentPath::GetChart => "/agent/chart/:token_id",
            AgentPath::GetSwapHistory => "/agent/swap-history/:token_id",
            AgentPath::GetMarket => "/agent/market/:token_id",
            AgentPath::GetMetrics => "/agent/metrics/:token_id",
            AgentPath::GetToken => "/agent/token/:token_id",
            AgentPath::GetTokenMetadata => "/agent/token/metadata/:token_id",
            AgentPath::GetHoldings => "/agent/holdings/:account_id",
            AgentPath::UploadImage => "/agent/token/image",
            AgentPath::UploadMetadata => "/agent/token/metadata",
            AgentPath::GetTokensCreated => "/agent/token/created/:account_id",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            AgentPath::GetChart => "/agent/chart/{token_id}",
            AgentPath::GetSwapHistory => "/agent/swap-history/{token_id}",
            AgentPath::GetMarket => "/agent/market/{token_id}",
            AgentPath::GetMetrics => "/agent/metrics/{token_id}",
            AgentPath::GetToken => "/agent/token/{token_id}",
            AgentPath::GetTokenMetadata => "/agent/token/metadata/{token_id}",
            AgentPath::GetHoldings => "/agent/holdings/{account_id}",
            AgentPath::UploadImage => "/agent/token/image",
            AgentPath::UploadMetadata => "/agent/token/metadata",
            AgentPath::GetTokensCreated => "/agent/token/created/{account_id}",
        }
    }
}
```

#### Step 1.2: Create Agent Module
**File:** `src/router/agent/mod.rs`

```rust
pub mod handler;
pub mod path;

use axum::{Router, extract::DefaultBodyLimit, routing::{get, post}};
use path::AgentPath;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(AgentPath::GetChart.as_str(), get(handler::get_chart))
        .route(AgentPath::GetSwapHistory.as_str(), get(handler::get_swap_history))
        .route(AgentPath::GetMarket.as_str(), get(handler::get_market))
        .route(AgentPath::GetMetrics.as_str(), get(handler::get_metrics))
        .route(AgentPath::GetToken.as_str(), get(handler::get_token))
        .route(AgentPath::GetTokenMetadata.as_str(), get(handler::get_token_metadata))
        .route(AgentPath::GetHoldings.as_str(), get(handler::get_holdings))
        .route(
            AgentPath::UploadImage.as_str(),
            post(handler::upload_image).layer(DefaultBodyLimit::max(5_000_000)),
        )
        .route(
            AgentPath::UploadMetadata.as_str(),
            post(handler::upload_metadata).layer(DefaultBodyLimit::max(5_000_000)),
        )
        .route(AgentPath::GetTokensCreated.as_str(), get(handler::get_tokens_created))
}
```

#### Step 1.3: Register in Router Module
**File:** `src/router/mod.rs` - Add: `pub mod agent;`

### Phase 2: Handler Implementation

#### Step 2.1: Create Handler File
**File:** `src/router/agent/handler.rs`

```rust
use axum::{
    Json,
    extract::{Path, Query, State},
    http::HeaderMap,
};
use bytes::Bytes;
use serde::Deserialize;
use tracing::{error, info, instrument};
use utoipa::IntoParams;

use crate::{
    result::{AppError, AppJsonResult},
    services::{
        metadata::MetadataService,
        token::{create::TokenCreatedService, detail::TokenService, metadata::TokenMetadataService},
        trading::{
            chart::ChartService, market::MarketService, metrics::MetricsService,
            position::PositionService, swap_history::SwapService,
        },
    },
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        metadata::{UploadImageResponse, UploadMetadataRequest, UploadMetadataResponse},
        profile::{CreatedTokensResponse, HoldTokenResponse},
        token::{TokenResponse, metadata::TokenMetadataResponse},
        trading::{
            chart::{BarResponse, GetBarsRequest},
            market::MarketResponse,
            metrics::{MetricsBatchResponse, TimeFrame},
            swap_history::{SwapQuery, TokenSwapResponse},
        },
    },
    utils::{valid_account_id, valid_token_id},
};

// ============================================================================
// Trading Data Handlers
// ============================================================================

#[utoipa::path(
    get,
    path = "/agent/chart/{token_id}",
    params(
        ("token_id" = String, Path, description = "Token contract address (0x...)"),
        ("resolution" = String, Query, description = "Chart resolution: 1, 5, 15, 30, 60, 240, 1D"),
        ("from" = i64, Query, description = "Start timestamp (Unix seconds)"),
        ("to" = i64, Query, description = "End timestamp (Unix seconds)"),
        ("countback" = Option<i32>, Query, description = "Max candles (default: 500)"),
        ("chart_type" = Option<String>, Query, description = "price, price_usd, market_cap, market_cap_usd")
    ),
    responses(
        (status = 200, description = "Chart data", body = BarResponse),
        (status = 400, description = "Invalid parameters"),
        (status = 401, description = "API key required"),
    ),
    security(("api_key" = [])),
    tag = "Agent"
)]
#[instrument(skip(state))]
pub async fn get_chart(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
    Query(query): Query<GetBarsRequest>,
) -> AppJsonResult<BarResponse> {
    let token_id = valid_token_id(&token_id).ok_or_else(|| {
        error!("Invalid token ID format: {}", token_id);
        AppError::BadRequest("Invalid token ID".to_string())
    })?;
    query.validate().map_err(AppError::BadRequest)?;
    let service = ChartService::new(state.postgres.clone(), state.redis.clone());
    Ok(Json(service.get_prices(&token_id, &query).await?))
}

#[utoipa::path(
    get,
    path = "/agent/swap-history/{token_id}",
    params(
        ("token_id" = String, Path, description = "Token contract address"),
        ("page" = Option<i64>, Query, description = "Page number (default: 1)"),
        ("limit" = Option<i64>, Query, description = "Items per page (default: 10)"),
        ("direction" = Option<String>, Query, description = "ASC or DESC"),
        ("trade_type" = Option<String>, Query, description = "BUY, SELL, or ALL")
    ),
    responses(
        (status = 200, description = "Swap history", body = TokenSwapResponse),
        (status = 400, description = "Invalid parameters"),
        (status = 401, description = "API key required"),
    ),
    security(("api_key" = [])),
    tag = "Agent"
)]
#[instrument(skip(state))]
pub async fn get_swap_history(
    Path(token_id): Path<String>,
    Query(query): Query<SwapQuery>,
    State(state): State<AppState>,
) -> AppJsonResult<TokenSwapResponse> {
    let token_id = valid_token_id(&token_id).ok_or_else(|| {
        error!("Invalid token ID format: {}", token_id);
        AppError::BadRequest("Invalid token ID".to_string())
    })?;
    query.validate().map_err(AppError::BadRequest)?;
    let service = SwapService::new(state.postgres.clone(), state.redis.clone());
    Ok(Json(service.get_swaps_by_token(&token_id, &query).await?))
}

#[utoipa::path(
    get,
    path = "/agent/market/{token_id}",
    params(("token_id" = String, Path, description = "Token contract address")),
    responses(
        (status = 200, description = "Market data", body = MarketResponse),
        (status = 400, description = "Invalid token ID"),
        (status = 401, description = "API key required"),
    ),
    security(("api_key" = [])),
    tag = "Agent"
)]
#[instrument(skip(state))]
pub async fn get_market(
    Path(token_id): Path<String>,
    State(state): State<AppState>,
) -> AppJsonResult<MarketResponse> {
    let token_id = valid_token_id(&token_id).ok_or_else(|| {
        error!("Invalid token ID format: {}", token_id);
        AppError::BadRequest("Invalid token ID".to_string())
    })?;
    let service = MarketService::new(state.postgres.clone(), state.redis.clone());
    Ok(Json(service.get_market(&token_id).await?))
}

fn deserialize_timeframes<'de, D>(deserializer: D) -> Result<Vec<TimeFrame>, D::Error>
where D: serde::Deserializer<'de> {
    use serde::de::Error;
    let s = String::deserialize(deserializer)?;
    s.split(',').map(|t| match t.trim() {
        "1" => Ok(TimeFrame::OneMinute),
        "5" => Ok(TimeFrame::FiveMinutes),
        "15" => Ok(TimeFrame::FifteenMinutes),
        "30" => Ok(TimeFrame::ThirtyMinutes),
        "60" => Ok(TimeFrame::OneHour),
        "240" => Ok(TimeFrame::FourHours),
        "1D" => Ok(TimeFrame::OneDay),
        _ => Err(D::Error::custom(format!("Invalid timeframe: {}", t)))
    }).collect()
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MetricsQuery {
    #[serde(deserialize_with = "deserialize_timeframes")]
    pub timeframes: Vec<TimeFrame>,
}

#[utoipa::path(
    get,
    path = "/agent/metrics/{token_id}",
    params(
        ("token_id" = String, Path, description = "Token contract address"),
        ("timeframes" = String, Query, description = "Comma-separated: 1,5,15,30,60,240,1D")
    ),
    responses(
        (status = 200, description = "Trading metrics", body = MetricsBatchResponse),
        (status = 400, description = "Invalid parameters"),
        (status = 401, description = "API key required"),
    ),
    security(("api_key" = [])),
    tag = "Agent"
)]
#[instrument(skip(state))]
pub async fn get_metrics(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
    Query(params): Query<MetricsQuery>,
) -> AppJsonResult<MetricsBatchResponse> {
    let token_id = valid_token_id(&token_id).ok_or_else(|| {
        error!("Invalid token ID format: {}", token_id);
        AppError::BadRequest("Invalid token ID".to_string())
    })?;
    let service = MetricsService::new(state.postgres.clone());
    Ok(Json(service.get_metrics(&token_id, params.timeframes).await?))
}

// ============================================================================
// Token Data Handlers
// ============================================================================

#[utoipa::path(
    get,
    path = "/agent/token/{token_id}",
    params(("token_id" = String, Path, description = "Token contract address")),
    responses(
        (status = 200, description = "Token info", body = TokenResponse),
        (status = 400, description = "Invalid token ID"),
        (status = 401, description = "API key required"),
        (status = 404, description = "Token not found"),
    ),
    security(("api_key" = [])),
    tag = "Agent"
)]
#[instrument(skip(state))]
pub async fn get_token(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
) -> AppJsonResult<TokenResponse> {
    let token_id = valid_token_id(&token_id).ok_or_else(|| {
        error!("Invalid token ID format: {}", token_id);
        AppError::BadRequest("Invalid token ID".to_string())
    })?;
    let service = TokenService::new(state.postgres.clone(), state.redis.clone());
    Ok(Json(service.get_token(&token_id).await?))
}

#[utoipa::path(
    get,
    path = "/agent/token/metadata/{token_id}",
    params(("token_id" = String, Path, description = "Token contract address")),
    responses(
        (status = 200, description = "Token metadata", body = TokenMetadataResponse),
        (status = 400, description = "Invalid token ID"),
        (status = 401, description = "API key required"),
        (status = 404, description = "Token not found"),
    ),
    security(("api_key" = [])),
    tag = "Agent"
)]
#[instrument(skip(state))]
pub async fn get_token_metadata(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
) -> AppJsonResult<TokenMetadataResponse> {
    let token_id = valid_token_id(&token_id).ok_or_else(|| {
        error!("Invalid token ID format: {}", token_id);
        AppError::BadRequest("Invalid token ID".to_string())
    })?;
    let service = TokenMetadataService::new(state.postgres.clone(), state.redis.clone());
    Ok(Json(service.get_token_metadata(&token_id).await?))
}

// ============================================================================
// Holdings Handler
// ============================================================================

#[utoipa::path(
    get,
    path = "/agent/holdings/{account_id}",
    params(
        ("account_id" = String, Path, description = "User's Ethereum address"),
        ("page" = Option<i64>, Query, description = "Page number"),
        ("limit" = Option<i64>, Query, description = "Items per page")
    ),
    responses(
        (status = 200, description = "Holdings", body = HoldTokenResponse),
        (status = 400, description = "Invalid account ID"),
        (status = 401, description = "API key required"),
    ),
    security(("api_key" = [])),
    tag = "Agent"
)]
#[instrument(skip(state))]
pub async fn get_holdings(
    Path(account_id): Path<String>,
    Query(query): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<HoldTokenResponse> {
    let account_id = valid_account_id(&account_id).ok_or_else(|| {
        error!("Invalid account ID format: {}", account_id);
        AppError::BadRequest("Invalid account ID".to_string())
    })?;
    let service = PositionService::new(state.postgres.clone(), state.redis.clone());
    Ok(Json(service.get_hold_token_by_account(&account_id, &query).await?))
}

// ============================================================================
// Token Creation Handlers
// ============================================================================

#[utoipa::path(
    post,
    path = "/agent/token/image",
    request_body(content = Vec<u8>, description = "Image binary (max 5MB)", content_type = "image/png"),
    responses(
        (status = 200, description = "Image uploaded", body = UploadImageResponse),
        (status = 400, description = "Invalid image"),
        (status = 401, description = "API key required"),
        (status = 413, description = "Image too large"),
    ),
    security(("api_key" = [])),
    tag = "Agent"
)]
pub async fn upload_image(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> AppJsonResult<UploadImageResponse> {
    info!("Agent: Image upload - {} bytes", body.len());
    let content_type = headers.get("content-type").and_then(|v| v.to_str().ok()).map(String::from);
    let service = MetadataService::new(state.postgres.clone(), state.redis.clone(), state.r2.clone());
    Ok(Json(service.process_and_upload_image(&body, &content_type).await?))
}

#[utoipa::path(
    post,
    path = "/agent/token/metadata",
    request_body = UploadMetadataRequest,
    responses(
        (status = 200, description = "Metadata uploaded", body = UploadMetadataResponse),
        (status = 400, description = "Invalid metadata or NSFW unknown"),
        (status = 401, description = "API key required"),
    ),
    security(("api_key" = [])),
    tag = "Agent"
)]
pub async fn upload_metadata(
    State(state): State<AppState>,
    Json(payload): Json<UploadMetadataRequest>,
) -> AppJsonResult<UploadMetadataResponse> {
    info!("Agent: Metadata upload for: {}", payload.name);
    let service = MetadataService::new(state.postgres.clone(), state.redis.clone(), state.r2.clone());
    let metadata = service.validate_metadata_request(&payload).await?;
    Ok(Json(service.upload_metadata(metadata).await?))
}

#[utoipa::path(
    get,
    path = "/agent/token/created/{account_id}",
    params(
        ("account_id" = String, Path, description = "Creator's address"),
        ("page" = Option<i64>, Query, description = "Page number"),
        ("limit" = Option<i64>, Query, description = "Items per page")
    ),
    responses(
        (status = 200, description = "Created tokens with rewards", body = CreatedTokensResponse),
        (status = 400, description = "Invalid account ID"),
        (status = 401, description = "API key required"),
    ),
    security(("api_key" = [])),
    tag = "Agent"
)]
#[instrument(skip(state))]
pub async fn get_tokens_created(
    Path(account_id): Path<String>,
    Query(pagination): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<CreatedTokensResponse> {
    let account_id = valid_account_id(&account_id).ok_or_else(|| {
        error!("Invalid account ID format: {}", account_id);
        AppError::BadRequest("Invalid account ID".to_string())
    })?;
    let service = TokenCreatedService::new(state.postgres.clone(), state.redis.clone());
    Ok(Json(service.get_tokens_created(&account_id, &pagination).await?))
}
```

### Phase 3: Main Application Integration

#### Step 3.1: Update main.rs

**Add import:**
```rust
use api_server::router::agent;
```

**Add to router chain:**
```rust
.merge(agent::router())
```

**Update timeout exemption in `method_based_timeout`:**
```rust
let is_upload_endpoint = path.starts_with("/metadata/image")
    || path.starts_with("/metadata/metadata")
    || path.starts_with("/agent/token/image");
```

**Add to OpenAPI paths:**
```rust
// ----------------Agent----------------
router::agent::handler::get_chart,
router::agent::handler::get_swap_history,
router::agent::handler::get_market,
router::agent::handler::get_metrics,
router::agent::handler::get_token,
router::agent::handler::get_token_metadata,
router::agent::handler::get_holdings,
router::agent::handler::upload_image,
router::agent::handler::upload_metadata,
router::agent::handler::get_tokens_created,
```

**Add to OpenAPI tags:**
```rust
(name = "Agent", description = "Agent API endpoints for AI integrations"),
```

## Query Parameters Reference

| Endpoint | Query Parameters |
|----------|------------------|
| GET /agent/chart/:token_id | resolution, from, to, countback?, chart_type? |
| GET /agent/swap-history/:token_id | page?, limit?, direction?, trade_type? |
| GET /agent/market/:token_id | (none) |
| GET /agent/metrics/:token_id | timeframes (comma-separated) |
| GET /agent/token/:token_id | (none) |
| GET /agent/token/metadata/:token_id | (none) |
| GET /agent/holdings/:account_id | page?, limit? |
| POST /agent/token/image | Body: raw bytes, Header: content-type |
| POST /agent/token/metadata | Body: UploadMetadataRequest JSON |
| GET /agent/token/created/:account_id | page?, limit? |

## Response Type Mappings

| Endpoint | Response Type | Location |
|----------|---------------|----------|
| GET /agent/chart | BarResponse | types/trading/chart.rs |
| GET /agent/swap-history | TokenSwapResponse | types/trading/swap_history.rs |
| GET /agent/market | MarketResponse | types/trading/market.rs |
| GET /agent/metrics | MetricsBatchResponse | types/trading/metrics.rs |
| GET /agent/token | TokenResponse | types/token/mod.rs |
| GET /agent/token/metadata | TokenMetadataResponse | types/token/metadata.rs |
| GET /agent/holdings | HoldTokenResponse | types/profile/mod.rs |
| POST /agent/token/image | UploadImageResponse | types/metadata/mod.rs |
| POST /agent/token/metadata | UploadMetadataResponse | types/metadata/mod.rs |
| GET /agent/token/created | CreatedTokensResponse | types/profile/mod.rs |

## File Summary

| File | Action |
|------|--------|
| `src/router/agent/path.rs` | Create |
| `src/router/agent/mod.rs` | Create |
| `src/router/agent/handler.rs` | Create |
| `src/router/mod.rs` | Add `pub mod agent;` |
| `src/main.rs` | Add router, timeout exemption, OpenAPI |

## Success Criteria

- [ ] All 10 agent endpoints respond correctly
- [ ] API key authentication enforced
- [ ] Rate limiting works (60 req/min)
- [ ] OpenAPI docs show Agent tag in Swagger UI
- [ ] Image upload has 5MB limit and timeout exemption
- [ ] Build passes
