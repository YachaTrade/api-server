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
        token::{create::TokenCreatedService, detail::TokenService, salt::SaltService},
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
        token::{
            TokenResponse,
            salt::{MineSaltRequest, MineSaltResponse},
        },
        trading::{
            chart::{BarResponse, GetBarsRequest},
            market::MarketResponse,
            metrics::{MetricsBatchResponse, TimeFrame},
            swap_history::{SwapQuery, TokenSwapResponse},
        },
    },
    utils::{valid_account_id, valid_existing_token_id},
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
    let token_id = valid_existing_token_id(&state, &token_id).await?;
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
    let token_id = valid_existing_token_id(&state, &token_id).await?;
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
    let token_id = valid_existing_token_id(&state, &token_id).await?;
    let service = MarketService::new(state.postgres.clone(), state.redis.clone());
    Ok(Json(service.get_market(&token_id).await?))
}

fn deserialize_timeframes<'de, D>(deserializer: D) -> Result<Vec<TimeFrame>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let s = String::deserialize(deserializer)?;
    s.split(',')
        .map(|t| match t.trim() {
            "1" => Ok(TimeFrame::OneMinute),
            "5" => Ok(TimeFrame::FiveMinutes),
            "15" => Ok(TimeFrame::FifteenMinutes),
            "30" => Ok(TimeFrame::ThirtyMinutes),
            "60" => Ok(TimeFrame::OneHour),
            "240" => Ok(TimeFrame::FourHours),
            "1D" => Ok(TimeFrame::OneDay),
            _ => Err(D::Error::custom(format!("Invalid timeframe: {}", t))),
        })
        .collect()
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
    let token_id = valid_existing_token_id(&state, &token_id).await?;
    let service = MetricsService::new(state.postgres.clone());
    Ok(Json(
        service.get_metrics(&token_id, params.timeframes).await?,
    ))
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
    let token_id = valid_existing_token_id(&state, &token_id).await?;
    let service = TokenService::new(state.postgres.clone(), state.redis.clone());
    Ok(Json(service.get_token(&token_id).await?))
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
    let response = service
        .get_hold_token_by_account(&account_id, &query)
        .await?;

    Ok(Json(response))
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
    let content_type = headers
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let service = MetadataService::new(
        state.postgres.clone(),
        state.redis.clone(),
        state.r2.clone(),
    );
    Ok(Json(
        service
            .process_and_upload_image(&body, &content_type)
            .await?,
    ))
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
    let service = MetadataService::new(
        state.postgres.clone(),
        state.redis.clone(),
        state.r2.clone(),
    );
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
        (status = 200, description = "Created tokens", body = CreatedTokensResponse),
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
    let response = service.get_tokens_created(&account_id, &pagination).await?;

    Ok(Json(response))
}

// ============================================================================
// Salt Mining Handler
// ============================================================================

#[utoipa::path(
    post,
    path = "/agent/salt",
    request_body = MineSaltRequest,
    responses(
        (status = 200, description = "Salt mined successfully", body = MineSaltResponse),
        (status = 400, description = "Invalid parameters"),
        (status = 401, description = "API key required"),
        (status = 408, description = "Request timeout - max iterations reached"),
    ),
    security(("api_key" = [])),
    tag = "Agent"
)]
#[instrument(skip(_state))]
pub async fn salt(
    State(_state): State<AppState>,
    Json(payload): Json<MineSaltRequest>,
) -> AppJsonResult<MineSaltResponse> {
    info!("Agent: Salt mining for creator: {}", payload.creator);
    payload.validate().map_err(AppError::BadRequest)?;
    let service = SaltService::new();
    Ok(Json(service.mine_salt(payload).await?))
}
