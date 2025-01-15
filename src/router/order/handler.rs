use axum::{
    extract::{Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{instrument, warn};
use utoipa::ToSchema;

use crate::{
    db::postgres::controller::{king::KingOfTheHillController, order::OrderController},
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{order::TokenOrderType, pagination::PaginationParams, response::OrderToken},
};

use super::path::OrderPath;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderMessage {
    pub order_type: TokenOrderType,
    pub order_token: Option<Vec<OrderToken>>,
    pub king_of_the_hill: Option<OrderToken>,
}

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
    let king_of_the_hill = KingOfTheHillController::new(state.postgres.clone())
        .get_latest_king_of_the_hill()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    let response = OrderMessage {
        order_type: TokenOrderType::CreationTime,
        order_token: Some(order_tokens),
        king_of_the_hill,
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

    let king_of_the_hill = KingOfTheHillController::new(state.postgres.clone())
        .get_latest_king_of_the_hill()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    let response = OrderMessage {
        order_type: TokenOrderType::MarketCap,
        order_token: Some(order_tokens),
        king_of_the_hill,
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
    let king_of_the_hill = KingOfTheHillController::new(state.postgres.clone())
        .get_latest_king_of_the_hill()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    let response = OrderMessage {
        order_type: TokenOrderType::LatestTrade,
        order_token: Some(order_tokens),
        king_of_the_hill,
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

/// Get tokens ordered by reply count
#[utoipa::path(
    get,
    path = OrderPath::ReplyCount.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Successfully retrieved tokens ordered by reply count", body = OrderMessage),
        (status = 400, description = "Bad request - Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Order"
)]
pub async fn get_reply_count_order(
    State(state): State<AppState>,
    Query(query): Query<PaginationParams>,
) -> AppJsonResult<OrderMessage> {
    // 캐시된 결과 확인
    if let Ok(cached_response) = state
        .redis
        .get_order_response(&TokenOrderType::ReplyCount)
        .await
    {
        return Ok(Json(cached_response));
    }

    let order_controller = OrderController::new(state.postgres.clone());
    let order_tokens = order_controller
        .get_order_tokens(TokenOrderType::ReplyCount, query)
        .await?;
    let king_of_the_hill = KingOfTheHillController::new(state.postgres.clone())
        .get_latest_king_of_the_hill()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    let response = OrderMessage {
        order_type: TokenOrderType::ReplyCount,
        order_token: Some(order_tokens),
        king_of_the_hill,
    };

    // 결과를 캐시에 저장
    if let Err(err) = state
        .redis
        .set_order_response(&TokenOrderType::ReplyCount, &response)
        .await
    {
        warn!(
            "Failed to set {:?} cache: {}",
            TokenOrderType::ReplyCount,
            err
        );
    }

    Ok(Json(response))
}

/// Get tokens ordered by latest reply
#[utoipa::path(
    get,
    path = OrderPath::LatestReply.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Successfully retrieved tokens ordered by latest reply", body = OrderMessage),
        (status = 400, description = "Bad request - Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Order"
)]
pub async fn get_latest_reply_order(
    State(state): State<AppState>,
    Query(query): Query<PaginationParams>,
) -> AppJsonResult<OrderMessage> {
    // 캐시된 결과 확인
    if let Ok(cached_response) = state
        .redis
        .get_order_response(&TokenOrderType::LatestReply)
        .await
    {
        return Ok(Json(cached_response));
    }

    let order_controller = OrderController::new(state.postgres.clone());
    let order_tokens = order_controller
        .get_order_tokens(TokenOrderType::LatestReply, query)
        .await?;
    let king_of_the_hill = KingOfTheHillController::new(state.postgres.clone())
        .get_latest_king_of_the_hill()
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;
    let response = OrderMessage {
        order_type: TokenOrderType::LatestReply,
        order_token: Some(order_tokens),
        king_of_the_hill,
    };

    // 결과를 캐시에 저장
    if let Err(err) = state
        .redis
        .set_order_response(&TokenOrderType::LatestReply, &response)
        .await
    {
        warn!(
            "Failed to set {:?} cache: {}",
            TokenOrderType::LatestReply,
            err
        );
    }

    Ok(Json(response))
}
