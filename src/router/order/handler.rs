use axum::{
    Json,
    extract::{Query, State},
};

use tracing::instrument;

use crate::{
    result::AppJsonResult,
    services::token::order::TokenOrderService,
    state::AppState,
    types::token::order::{OrderQuery, OrderTokenResponse, TokenOrderType},
};

use super::path::OrderPath;

/// Get tokens ordered by creation time
#[utoipa::path(
    get,
    path = OrderPath::Creationtime.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page"),
        ("direction" = Option<String>, Query, description = "Direction of pagination (ASC or DESC) Default:DESC"),
        ("is_nsfw" = Option<bool>, Query, description = "Filter NSFW tokens (default: false)")
    ),
    responses(
        (status = 200, description = "Successfully retrieved tokens ordered by creation time", body = OrderTokenResponse),
        (status = 400, description = "Bad request - Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Order"
)]
#[instrument(skip(state))]
pub async fn get_creation_time_order(
    State(state): State<AppState>,
    Query(query): Query<OrderQuery>,
) -> AppJsonResult<OrderTokenResponse> {
    let service = TokenOrderService::new(state.postgres.clone(), state.redis.clone());
    let response = service
        .get_order(TokenOrderType::CreationTime, &query)
        .await?;

    Ok(Json(response))
}

/// Get tokens ordered by market cap
#[utoipa::path(
    get,
    path = OrderPath::MarketCap.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page"),
        ("direction" = Option<String>, Query, description = "Direction of pagination (ASC or DESC) Default:DESC"),
        ("is_nsfw" = Option<bool>, Query, description = "Filter NSFW tokens (default: false)")
    ),
    responses(
        (status = 200, description = "Successfully retrieved tokens ordered by market cap", body = OrderTokenResponse),
        (status = 400, description = "Bad request - Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Order"
)]
#[instrument(skip(state))]
pub async fn get_market_cap_order(
    State(state): State<AppState>,
    Query(query): Query<OrderQuery>,
) -> AppJsonResult<OrderTokenResponse> {
    let service = TokenOrderService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_order(TokenOrderType::MarketCap, &query).await?;

    Ok(Json(response))
}

/// Get tokens ordered by latest trade
#[utoipa::path(
    get,
    path = OrderPath::LatestTrade.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page"),
        ("direction" = Option<String>, Query, description = "Direction of pagination (ASC or DESC) Default:DESC"),
        ("is_nsfw" = Option<bool>, Query, description = "Filter NSFW tokens (default: false)")
    ),
    responses(
        (status = 200, description = "Successfully retrieved tokens ordered by latest trade", body = OrderTokenResponse),
        (status = 400, description = "Bad request - Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Order"
)]
#[instrument(skip(state))]
pub async fn get_latest_trade_order(
    State(state): State<AppState>,
    Query(query): Query<OrderQuery>,
) -> AppJsonResult<OrderTokenResponse> {
    let service = TokenOrderService::new(state.postgres.clone(), state.redis.clone());
    let response = service
        .get_order(TokenOrderType::LatestTrade, &query)
        .await?;

    Ok(Json(response))
}

/// Get hackathon tokens ordered by market cap
#[utoipa::path(
    get,
    path = OrderPath::Hackathon.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items per page"),
        ("direction" = Option<String>, Query, description = "Direction of pagination (ASC or DESC) Default:DESC"),
        ("is_nsfw" = Option<bool>, Query, description = "Filter NSFW tokens (default: false)")
    ),
    responses(
        (status = 200, description = "Successfully retrieved hackathon tokens ordered by market cap", body = OrderTokenResponse),
        (status = 400, description = "Bad request - Invalid pagination parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Order"
)]
#[instrument(skip(state))]
pub async fn get_hackathon_order(
    State(state): State<AppState>,
    Query(query): Query<OrderQuery>,
) -> AppJsonResult<OrderTokenResponse> {
    let service = TokenOrderService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_order(TokenOrderType::Hackathon, &query).await?;

    Ok(Json(response))
}
