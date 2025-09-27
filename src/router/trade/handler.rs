use axum::{
    Json,
    extract::{Path, Query, State},
};
use std::time::Instant;

use serde::Deserialize;
use serde_json::json;
use tracing::{error, info, instrument, warn};
use utoipa::IntoParams;

use crate::{
    result::{AppError, AppJsonResult},
    router::trade::path::TradePath,
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        trading::{
            chart::{BarResponse, ChartController, GetBarsRequest},
            market::{Market, MarketController},
            metrics::{
                MetricsController, TimeFrame, TokenTradingMetrics, TokenTradingMetricsBatch,
            },
            position::{PositionController, TokenHolderResponse},
            price::{PriceController, PriceResponse},
            swap_history::{SwapController, SwapQuery, TokenSwapResponse},
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

    if let Ok(cached_response) = state.redis.get_token_swap_history(&token_id, &query).await {
        return Ok(Json(cached_response));
    }

    let response = SwapController::new(state.postgres.clone())
        .get_swaps_by_token(&token_id, &query)
        .await
        .map_err(|err| {
            error!(
                "Failed to get swap history: token_id: {:?}, error: {}",
                token_id, err
            );
            AppError::InternalError(err.to_string())
        })?;

    if let Err(err) = state
        .redis
        .set_token_swap_history(&token_id, &response, &query)
        .await
    {
        warn!("Failed to set token swap history cache: {}", err);
    }

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
    if let Ok(cached_response) = state
        .redis
        .get_token_holder_response(&token_id, &params)
        .await
    {
        return Ok(Json(cached_response));
    }
    let response = PositionController::new(state.postgres.clone())
        .get_holders_by_token(&token_id, &params)
        .await
        .map_err(|err| {
            error!(
                "Failed to get token holders: token_id: {}, error: {}",
                token_id, err
            );
            AppError::InternalError(err.to_string())
        })?;
    if let Err(err) = state
        .redis
        .set_token_holder_response(&token_id, &response, &params)
        .await
    {
        warn!("Failed to set token holder cache: {}", err);
    }
    Ok(Json(response))
}

///Get market information for a token
#[utoipa::path(
    get,
    path = TradePath::GetMarket.docs_str(),
    responses(
        (status = 200, description = "Success", body = Market),
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
) -> AppJsonResult<Market> {
    if !valid_evm_address(&token_id) {
        error!("Invalid token ID format: {}", token_id);
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }

    // Try to get from cache first
    if let Ok(cached_response) = state.redis.get_market(&token_id).await {
        return Ok(Json(cached_response));
    }

    let response = MarketController::new(state.postgres.clone())
        .get_market_by_token(&token_id)
        .await
        .map_err(|err| {
            error!(
                "Failed to get market information: token_id: {}, error: {}",
                token_id, err
            );
            AppError::InternalError(err.to_string())
        })?;

    // Cache the response
    if let Err(e) = state.redis.set_market(&token_id, &response).await {
        error!("Failed to set market cache: {}", e);
    }

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
        ("countback" = Option<i32>, Query, description = "Maximum number of candles to return (default: 500)")
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

    // 캐시에서 먼저 데이터 조회
    let cache_result = state.redis.get_prices(&token_id, &query).await;

    if let Ok(Some(cached_data)) = cache_result {
        return Ok(Json(cached_data));
    }

    let chart_controller = ChartController::new(state.postgres.clone());

    // 요청에서 필요한 매개변수 추출
    let token_id = token_id;

    // 차트 데이터 가져오기
    let bar_data = chart_controller
        .get_prices(&token_id, &query)
        .await
        .map_err(|err| {
            let err_msg = format!(
                "Failed to get price chart data: token_id: {}, error: {}",
                token_id, err
            );
            error!("{}", err_msg);
            AppError::InternalError(err_msg)
        })?;

    // 조회 결과를 캐시에 저장
    if let Err(err) = state.redis.set_prices(&token_id, &query, &bar_data).await {
        error!("Failed to cache bar data response: {}", err);
    }

    Ok(Json(bar_data))
}

///Get price for a token
#[utoipa::path(
    get,
    path = TradePath::GetPrice.docs_str(),
    responses(
        (status = 200, description = "Success", body = PriceResponse),
        (status = 400, description = "Invalid token ID"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("token_id" = String, Path, description = "Token ID")
    ),
    tag = "Trade"
)]
#[instrument(skip(state))]
pub async fn get_price(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> AppJsonResult<PriceResponse> {
    let price_controller = PriceController::new(state.postgres.clone());
    let price_response = price_controller.get_price(&token).await.map_err(|err| {
        error!("Failed to get price: token: {}, error: {}", token, err);
        AppError::InternalError(format!("Failed to get price: {}", err))
    })?;
    Ok(Json(price_response))
}

#[derive(Debug, Deserialize, IntoParams)]
pub struct MetricsQuery {
    /// Timeframe for metrics (1, 5, 15, 30, 60, 4H, D, W, M)
    pub timeframe: TimeFrame,
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

/// Get trading metrics for a token within a specific timeframe
#[utoipa::path(
    get,
    path = TradePath::GetMetrics.docs_str(),
    params(
        ("token_id" = String, Path, description = "Token ID"),
        MetricsQuery
    ),
    responses(
        (status = 200, description = "Trading metrics retrieved successfully", body = TokenTradingMetrics),
        (status = 400, description = "Bad request - Invalid token_id or timeframe"),
        (status = 500, description = "Internal server error - Database query failed")
    ),
    tag = "Trade"
)]
pub async fn get_metrics(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
    Query(params): Query<MetricsQuery>,
) -> AppJsonResult<TokenTradingMetrics> {
    let start_time = Instant::now();
    info!(
        "🚀 Getting trading metrics for token: {}, timeframe: {}",
        token_id,
        params.timeframe.to_display_string()
    );

    if !valid_evm_address(&token_id) {
        error!("Invalid token ID format: {}", token_id);
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }

    let metrics_controller = MetricsController::new(state.postgres.clone());

    let metrics = metrics_controller
        .trading_metrics(&token_id, params.timeframe)
        .await
        .map_err(|e| {
            error!("Failed to get trading metrics: {}", e);
            AppError::InternalError(format!("Failed to get trading metrics: {}", e))
        })?;

    let elapsed = start_time.elapsed();
    info!(
        "🎉 Trading metrics retrieved successfully in {:?} - Token: {}, Volume: {}",
        elapsed, token_id, metrics.volume
    );

    Ok(Json(metrics))
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
        (status = 200, description = "Trading metrics retrieved successfully for multiple timeframes", body = Vec<TokenTradingMetrics>),
        (status = 400, description = "Bad request - Invalid token_id or timeframes"),
        (status = 500, description = "Internal server error - Database query failed")
    ),
    tag = "Trade"
)]
pub async fn get_metrics_batch(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
    Query(params): Query<MetricsBatchQuery>,
) -> AppJsonResult<TokenTradingMetricsBatch> {
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

    let metrics_controller = MetricsController::new(state.postgres.clone());

    let metrics_batch = metrics_controller
        .trading_metrics_batch(&token_id, params.timeframes)
        .await
        .map_err(|e| {
            error!("Failed to get batch trading metrics: {}", e);
            AppError::InternalError(format!("Failed to get batch trading metrics: {}", e))
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
