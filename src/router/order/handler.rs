use axum::{
    extract::{Query, State},
    Json,
};

use tracing::{instrument, warn};

use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        token::order::{OrderController, OrderMessage, TokenOrderType},
    },
};

use super::path::OrderPath;

/// Get tokens ordered by creation time
#[utoipa::path(
    get,
    path = OrderPath::Creationtime.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Successfully retrieved tokens ordered by creation time", body = OrderMessage),
        (status = 400, description = "Bad request - Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Order"
)]
#[instrument(skip(state))]
pub async fn get_creation_time_order(
    State(state): State<AppState>,
    Query(query): Query<PaginationParams>,
) -> AppJsonResult<OrderMessage> {
    // 캐시된 결과 확인
    if let Ok(cached_response) = state
        .redis
        .get_order_response(&TokenOrderType::CreationTime)
        .await
    {
        return Ok(Json(cached_response));
    }

    let order_controller = OrderController::new(state.postgres.clone());
    let order_tokens = order_controller
        .get_order_tokens(TokenOrderType::CreationTime, query)
        .await?;
    let king_of_the_hill = OrderController::new(state.postgres.clone())
        .get_latest_king_of_the_hill()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    let total_count = OrderController::new(state.postgres.clone())
        .get_total_count()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    let response = OrderMessage {
        order_type: TokenOrderType::CreationTime,
        order_token: Some(order_tokens),
        king_of_the_hill,
        total_count,
    };

    // 결과를 캐시에 저장
    if let Err(err) = state
        .redis
        .set_order_response(&TokenOrderType::CreationTime, &response)
        .await
    {
        warn!(
            "Failed to set {:?} cache: {}",
            TokenOrderType::CreationTime,
            err
        );
    }

    Ok(Json(response))
}

/// Get tokens ordered by market cap
#[utoipa::path(
    get,
    path = OrderPath::MarketCap.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Successfully retrieved tokens ordered by market cap", body = OrderMessage),
        (status = 400, description = "Bad request - Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Order"
)]
#[instrument(skip(state))]
pub async fn get_market_cap_order(
    State(state): State<AppState>,
    Query(query): Query<PaginationParams>,
) -> AppJsonResult<OrderMessage> {
    // 캐시된 결과 확인
    if let Ok(cached_response) = state
        .redis
        .get_order_response(&TokenOrderType::MarketCap)
        .await
    {
        return Ok(Json(cached_response));
    }

    let order_controller = OrderController::new(state.postgres.clone());
    let order_tokens = order_controller
        .get_order_tokens(TokenOrderType::MarketCap, query)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    let king_of_the_hill = OrderController::new(state.postgres.clone())
        .get_latest_king_of_the_hill()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    let total_count = OrderController::new(state.postgres.clone())
        .get_total_count()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    let response = OrderMessage {
        order_type: TokenOrderType::MarketCap,
        order_token: Some(order_tokens),
        king_of_the_hill,
        total_count,
    };

    // 결과를 캐시에 저장
    if let Err(err) = state
        .redis
        .set_order_response(&TokenOrderType::MarketCap, &response)
        .await
    {
        warn!(
            "Failed to set {:?} cache: {}",
            TokenOrderType::MarketCap,
            err
        );
    }

    Ok(Json(response))
}

/// Get tokens ordered by latest trade
#[utoipa::path(
    get,
    path = OrderPath::LatestTrade.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Successfully retrieved tokens ordered by latest trade", body = OrderMessage),
        (status = 400, description = "Bad request - Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Order"
)]
#[instrument(skip(state))]
pub async fn get_latest_trade_order(
    State(state): State<AppState>,
    Query(query): Query<PaginationParams>,
) -> AppJsonResult<OrderMessage> {
    // 캐시된 결과 확인
    if let Ok(cached_response) = state
        .redis
        .get_order_response(&TokenOrderType::LatestTrade)
        .await
    {
        return Ok(Json(cached_response));
    }

    let order_controller = OrderController::new(state.postgres.clone());
    let order_tokens = order_controller
        .get_order_tokens(TokenOrderType::LatestTrade, query)
        .await?;
    let king_of_the_hill = OrderController::new(state.postgres.clone())
        .get_latest_king_of_the_hill()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    let total_count = OrderController::new(state.postgres.clone())
        .get_total_count()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    let response = OrderMessage {
        order_type: TokenOrderType::LatestTrade,
        order_token: Some(order_tokens),
        king_of_the_hill,
        total_count,
    };

    // 결과를 캐시에 저장
    if let Err(err) = state
        .redis
        .set_order_response(&TokenOrderType::LatestTrade, &response)
        .await
    {
        warn!(
            "Failed to set {:?} cache: {}",
            TokenOrderType::LatestTrade,
            err
        );
    }

    Ok(Json(response))
}
