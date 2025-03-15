use axum::{
    extract::{Path, Query, State},
    Json,
};
use tracing::{error, info, instrument, warn};

use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        trading::{
            chart::{ChartController, ChartInterval, ChartQuery, ChartResponse},
            market::{Market, MarketController},
            position::{PositionController, TokenHolderResponse},
            swap_history::{SwapController, TokenSwapResponse},
        },
    },
    utils::valid_evm_address,
};

use super::path::TradePath;

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
        ("limit" = i64, Query, description = "Number of items per page")
    ),
    tag = "Trade"
)]
#[instrument(skip(state))]
pub async fn get_swap_history(
    Path(token_id): Path<String>,
    Query(params): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<TokenSwapResponse> {
    if !valid_evm_address(&token_id) {
        error!("Invalid token ID format: {}", token_id);
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }

    if let Ok(cached_response) = state
        .trade_redis
        .get_token_swap_history(&token_id, &params)
        .await
    {
        return Ok(Json(cached_response));
    }

    let response = SwapController::new(state.postgres.clone())
        .get_swaps_by_token(&token_id, &params)
        .await
        .map_err(|err| {
            error!(
                "Failed to get swap history: token_id: {}, error: {}",
                token_id, err
            );
            AppError::InternalError(err.to_string())
        })?;
    if let Err(err) = state
        .trade_redis
        .set_token_swap_history(&token_id, &response, &params)
        .await
    {
        warn!("Failed to set token swap history cache: {}", err);
    }
    info!(
        "Get Swap History: token_id: {}, response: {:?}",
        token_id, response
    );
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
        .trade_redis
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
        .trade_redis
        .set_token_holder_response(&token_id, &response, &params)
        .await
    {
        warn!("Failed to set token holder cache: {}", err);
    }
    info!(
        "Get Token Holders: token_id: {}, response: {:?}",
        token_id, response
    );
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
    info!(
        "Get Market Information: token_id: {}, response: {:?}",
        token_id, response
    );
    Ok(Json(response))
}

///Get Chart for a token
#[utoipa::path(
    get,
    path = TradePath::GetChart.docs_str(),
    responses(
        (status = 200, description = "Success", body = ChartResponse),
        (status = 404, description = "Chart not found"),
        (status = 500, description = "Internal server error")
    ),
    params(
        ("token" = String, Path, description = "Token ID"),
        ("interval" = String, Query, description = "Chart interval (1m, 5m, 15m, 30m, 1h, 4h, 1d, 1w)"),
        ("base_timestamp" = i64, Query, description = "Base timestamp")
    ),
    tag = "Trade",
)]
#[instrument(skip(state))]
pub async fn get_chart(
    State(state): State<AppState>,
    Path(token): Path<String>,
    Query(query): Query<ChartQuery>,
) -> AppJsonResult<ChartResponse> {
    let chart_interval = ChartInterval::from_str(&query.interval).map_err(|err| {
        error!("Invalid chart interval: {}", err);
        AppError::BadRequest(format!("Invalid chart interval: {}", err))
    })?;

    let chart_controller = ChartController::new(state.postgres.clone());
    if let Ok(cached_response) = state.trade_redis.get_chart_response(&token, &query).await {
        return Ok(Json(cached_response));
    }
    let chart_response = chart_controller
        .get_chart(&token, chart_interval, query.base_timestamp)
        .await
        .map_err(|err| {
            error!("Failed to get chart: token: {}, error: {}", token, err);
            AppError::InternalError(format!("Failed to get chart: {}", err))
        })?;

    if let Err(err) = state
        .trade_redis
        .set_chart_response(&token, &query, &chart_response)
        .await
    {
        warn!("Failed to set chart cache: {}", err);
    }
    info!(
        "Get Chart: token: {}, response: {:?}",
        token, chart_response
    );
    Ok(Json(chart_response))
}
