use axum::{
    Json,
    extract::{Path, Query, State},
};
use std::time::Instant;

use serde::Deserialize;
use tracing::{error, info, instrument};
use utoipa::IntoParams;

use crate::{
    result::{AppError, AppJsonResult},
    router::trade::path::TradePath,
    services::trading::{
        chart::ChartService, market::MarketService, metrics::MetricsService,
        position::PositionService, swap_history::SwapService,
    },
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        trading::{
            chart::{BarResponse, GetBarsRequest},
            market::MarketResponse,
            metrics::{MetricsBatchResponse, TimeFrame},
            position::TokenHolderResponse,
            swap_history::{SwapQuery, TokenSwapResponse},
        },
    },
    utils::valid_evm_address,
};

///Get swap history for a token
#[utoipa::path(
    get,
    path = TradePath::GetSwapHistory.docs_str(),
    responses(
        (status = 200, description = "Success", body = TokenSwapResponse),
        (status = 400, description = "Invalid token ID"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("token_id" = String, Path, description = "Token ID"),
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page"),
        ("direction" = Option<String>, Query, description = "Sort direction (ASC or DESC) Default DESC"),
        ("min_volume" = Option<String>, Query, description = "Minimum volume filter (1 mon = 1000000000000000000)"),
        ("account_id" = Option<String>, Query, description = "Account ID for own trades filter"),
        ("trade_type" = Option<String>, Query, description = "Trade type filter: (BUY, SELL) Default ALL")
    ),
    tag = "Trade"
)]
#[instrument(skip(state))]
pub async fn get_swap_history(
    Path(token_id): Path<String>,
    Query(query): Query<SwapQuery>,
    State(state): State<AppState>,
) -> AppJsonResult<TokenSwapResponse> {
    if !valid_evm_address(&token_id) {
        error!("Invalid token ID format: {:?}", token_id);
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }
    query.validate().map_err(|e| {
        error!("Invalid filter parameters: {}", e);
        AppError::BadRequest(e)
    })?;

    let swap_service = SwapService::new(state.postgres.clone(), state.redis.clone());
    let response = swap_service.get_swaps_by_token(&token_id, &query).await?;

    Ok(Json(response))
}

///Get token holders
#[utoipa::path(
    get,
    path = TradePath::GetHolder.docs_str(),
    responses(
        (status = 200, description = "Success", body = TokenHolderResponse),
        (status = 400, description = "Invalid token ID"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("token_id" = String, Path, description = "Token ID"),
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page")
    ),
    tag = "Trade"
)]
#[instrument(skip(state))]
pub async fn get_holder(
    Path(token_id): Path<String>,
    Query(params): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<TokenHolderResponse> {
    if !valid_evm_address(&token_id) {
        error!("Invalid token ID format: {}", token_id);
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }
    let position_service = PositionService::new(state.postgres.clone(), state.redis.clone());
    let response = position_service
        .get_holders_by_token(&token_id, &params)
        .await?;
    Ok(Json(response))
}

///Get market information for a token
#[utoipa::path(
    get,
    path = TradePath::GetMarket.docs_str(),
    responses(
        (status = 200, description = "Success", body = MarketResponse),
        (status = 400, description = "Invalid token ID"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("token_id" = String, Path, description = "Token ID")
    ),
    tag = "Trade"
)]
#[instrument(skip(state))]
pub async fn get_market(
    Path(token_id): Path<String>,
    State(state): State<AppState>,
) -> AppJsonResult<MarketResponse> {
    if !valid_evm_address(&token_id) {
        error!("Invalid token ID format: {}", token_id);
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }

    let market_service = MarketService::new(state.postgres.clone(), state.redis.clone());
    let response = market_service.get_market(&token_id).await?;

    Ok(Json(response))
}

///Get Chart for a token
#[utoipa::path(
    get,
    path = TradePath::GetChart.docs_str(),
    params(
        ("token_id" = String, Path, description = "Token ID"),
        ("resolution" = String, Query, description = "Chart resolution (1, 5, 15, 30, 60/1H, 4H, D, W, M)"),
        ("from" = i64, Query, description = "Start timestamp (seconds)"),
        ("to" = i64, Query, description = "End timestamp (seconds)"),
        ("countback" = Option<i32>, Query, description = "Maximum number of candles to return (default: 500)"),
        ("chart_type" = Option<String>, Query, description = "Chart type: price (MON/TOKEN), price_usd (USD price), market_cap (MON market cap), market_cap_usd (USD market cap). Default: price")
    ),
    responses(
        (status = 200, description = "Success", body = BarResponse),
        (status = 400, description = "Invalid token address or request parameters"),
        (status = 404, description = "Token not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Trade"
)]
pub async fn get_prices(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
    Query(query): Query<GetBarsRequest>,
) -> AppJsonResult<BarResponse> {
    if !valid_evm_address(&token_id) {
        error!("Invalid token ID format: {}", token_id);
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }

    let chart_service = ChartService::new(state.postgres.clone(), state.redis.clone());
    let bar_data = chart_service.get_prices(&token_id, &query).await?;

    Ok(Json(bar_data))
}


#[derive(Debug, Deserialize, IntoParams)]
pub struct MetricsBatchQuery {
    /// Comma-separated timeframes string (e.g., "D,1H,5" or "D,W,M")
    /// Available values: 1, 5, 15, 30, 60, 4H, D, W, M
    #[serde(deserialize_with = "deserialize_comma_separated_timeframes")]
    pub timeframes: Vec<TimeFrame>,
}

/// Deserialize comma-separated timeframes string into Vec<TimeFrame>
fn deserialize_comma_separated_timeframes<'de, D>(
    deserializer: D,
) -> Result<Vec<TimeFrame>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    let s = String::deserialize(deserializer)?;
    let mut timeframes = Vec::new();

    for timeframe_str in s.split(',') {
        let trimmed = timeframe_str.trim();
        if !trimmed.is_empty() {
            match trimmed {
                "1" => timeframes.push(TimeFrame::OneMinute),
                "5" => timeframes.push(TimeFrame::FiveMinutes),
                "15" => timeframes.push(TimeFrame::FifteenMinutes),
                "30" => timeframes.push(TimeFrame::ThirtyMinutes),
                "60" => timeframes.push(TimeFrame::OneHour),
                "1H" => timeframes.push(TimeFrame::OneHour),
                "4H" => timeframes.push(TimeFrame::FourHours),
                "D" => timeframes.push(TimeFrame::OneDay),
                "W" => timeframes.push(TimeFrame::OneWeek),
                "M" => timeframes.push(TimeFrame::OneMonth),
                _ => return Err(D::Error::custom(format!("Invalid timeframe: {}", trimmed))),
            }
        }
    }

    if timeframes.is_empty() {
        return Err(D::Error::custom("At least one timeframe is required"));
    }

    Ok(timeframes)
}

/// Get trading metrics for multiple timeframes for a token
#[utoipa::path(
    get,
    path = TradePath::GetMetricsBatch.docs_str(),
    params(
        ("token_id" = String, Path, description = "Token ID"),
        ("timeframes" = String, Query, description = "Comma-separated timeframes (e.g., 'D,1H,5' or 'D,W,M'). Available values: 1, 5, 15, 30, 60, 4H, D, W, M", example = "D,1H,5")
    ),
    responses(
        (status = 200, description = "Trading metrics retrieved successfully for multiple timeframes", body = MetricsBatchResponse),
        (status = 400, description = "Bad request - Invalid token_id or timeframes"),
        (status = 500, description = "Internal server error - Database query failed")
    ),
    tag = "Trade"
)]
pub async fn get_metrics_batch(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
    Query(params): Query<MetricsBatchQuery>,
) -> AppJsonResult<MetricsBatchResponse> {
    let start_time = Instant::now();
    info!(
        "🚀 Getting batch trading metrics for token: {}, timeframes: {:?}",
        token_id,
        params
            .timeframes
            .iter()
            .map(|tf| tf.to_display_string())
            .collect::<Vec<_>>()
    );

    if !valid_evm_address(&token_id) {
        error!("Invalid token ID format: {}", token_id);
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }

    if params.timeframes.is_empty() {
        return Err(AppError::BadRequest(
            "At least one timeframe is required".to_string(),
        ));
    }

    let metrics_service = MetricsService::new(state.postgres.clone());

    let metrics_batch = metrics_service
        .get_metrics_batch(&token_id, params.timeframes)
        .await
        .map_err(|err| {
            error!("Failed to get batch trading metrics: {:?}", err);
            err
        })?;

    let elapsed = start_time.elapsed();
    info!(
        "🎉 Batch trading metrics retrieved successfully in {:?} - Token: {}, Count: {}",
        elapsed,
        token_id,
        metrics_batch.metrics.len()
    );

    Ok(Json(metrics_batch))
}
