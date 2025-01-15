use axum::{
    extract::{Query, State},
    Json,
};
use serde::Serialize;
use utoipa::ToSchema;

use crate::{
    db::postgres::controller::order::OrderController,
    result::AppJsonResult,
    state::AppState,
    types::{order::TokenOrderType, pagination::PaginationParams, response::OrderToken},
};

use super::path::OrderPath;

#[derive(Debug, Clone, Serialize, ToSchema)]
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
        ("size" = Option<i64>, Query, description = "Number of items per page")
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
    let order_controller = OrderController::new(state.postgres.clone());
    let order_tokens = order_controller
        .get_order_tokens(TokenOrderType::CreationTime, query)
        .await?;
    Ok(Json(OrderMessage {
        order_type: TokenOrderType::CreationTime,
        order_token: Some(order_tokens),
        king_of_the_hill: None,
    }))
}

/// Get tokens ordered by market cap
#[utoipa::path(
    get,
    path = OrderPath::MarketCap.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("size" = Option<i64>, Query, description = "Number of items per page")
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
    let order_controller = OrderController::new(state.postgres.clone());
    let order_tokens = order_controller
        .get_order_tokens(TokenOrderType::MarketCap, query)
        .await?;
    Ok(Json(OrderMessage {
        order_type: TokenOrderType::MarketCap,
        order_token: Some(order_tokens),
        king_of_the_hill: None,
    }))
}

/// Get tokens ordered by latest trade
#[utoipa::path(
    get,
    path = OrderPath::LatestTrade.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("size" = Option<i64>, Query, description = "Number of items per page")
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
    let order_controller = OrderController::new(state.postgres.clone());
    let order_tokens = order_controller
        .get_order_tokens(TokenOrderType::LatestTrade, query)
        .await?;
    Ok(Json(OrderMessage {
        order_type: TokenOrderType::LatestTrade,
        order_token: Some(order_tokens),
        king_of_the_hill: None,
    }))
}

/// Get tokens ordered by reply count
#[utoipa::path(
    get,
    path = OrderPath::ReplyCount.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("size" = Option<i64>, Query, description = "Number of items per page")
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
    let order_controller = OrderController::new(state.postgres.clone());
    let order_tokens = order_controller
        .get_order_tokens(TokenOrderType::ReplyCount, query)
        .await?;
    Ok(Json(OrderMessage {
        order_type: TokenOrderType::ReplyCount,
        order_token: Some(order_tokens),
        king_of_the_hill: None,
    }))
}

/// Get tokens ordered by latest reply
#[utoipa::path(
    get,
    path = OrderPath::LatestReply.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("size" = Option<i64>, Query, description = "Number of items per page")
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
    let order_controller = OrderController::new(state.postgres.clone());
    let order_tokens = order_controller
        .get_order_tokens(TokenOrderType::LatestReply, query)
        .await?;
    Ok(Json(OrderMessage {
        order_type: TokenOrderType::LatestReply,
        order_token: Some(order_tokens),
        king_of_the_hill: None,
    }))
}
