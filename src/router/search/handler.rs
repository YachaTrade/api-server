use axum::extract::{Path, Query};
use axum::{extract::State, Json};
use serde::Deserialize;

use tracing::{instrument, warn};

use utoipa::{schema, ToSchema};

use crate::result::{AppError, AppJsonResult};
use crate::state::AppState;
use crate::types::common::pagination::PaginationParams;
use crate::types::token::order::{OrderController, SearchResponse, TokenOrderType};

use super::path::SearchPath;

#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({
    "order_type": "latest_trade",
    "limit": 10,
    "offset": 0
}))]
pub struct SearchTokenQuery {
    #[schema(example = "latest_trade")]
    pub order_type: Option<String>,
    #[schema(example = 10)]
    pub limit: Option<i64>,
    #[schema(example = 0)]
    pub offset: Option<i64>,
}

/// Search token by name, symbol, or token address
#[utoipa::path(
    get,
    path = SearchPath::Search.docs_str(),
    params(
        ("token" = String, Path, description = "Token name, symbol, or address to search for", example = "PUMP"),
        ("order_type" = Option<String>, Query, description = "Order type for results (latest_trade, latest_reply, latest_create)", example = "latest_trade"),
        ("limit" = Option<i64>, Query, description = "Number of results to return", example = 10),
        ("offset" = Option<i64>, Query, description = "Number of results to skip", example = 0)
    ),
    responses(
        (status = 200, description = "Search tokens successfully", body = SearchResponse),
        (status = 400, description = "Bad request - Invalid parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Search"
)]
#[instrument(skip(state))]
pub async fn search_token(
    Path(token): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<SearchTokenQuery>,
) -> AppJsonResult<SearchResponse> {
    if token.is_empty() {
        warn!("Empty token search query");
        return Err(AppError::BadRequest("Empty token search query".to_string()));
    }

    // 캐시된 결과 확인
    if let Ok(cached_response) = state.redis.get_search_response(&token).await {
        return Ok(Json(cached_response));
    }

    let order_controller = OrderController::new(state.postgres.clone());
    let order_type = query.order_type.unwrap_or("latest_trade".to_string());
    let pagination = PaginationParams {
        page: query.offset.unwrap_or(1),
        limit: query.limit.unwrap_or(10),
    };
    let response = order_controller
        .search_order_tokens(&token, order_type, pagination)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    // 결과를 캐시에 저장
    if let Err(err) = state.redis.set_search_response(&token, &response).await {
        warn!("Failed to cache search response: {}", err);
    }

    Ok(Json(response))
}
