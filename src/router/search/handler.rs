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
    "sort_by": "market_cap",
    "page": 1,
    "limit": 10
}))]
pub struct SearchTokenQuery {
    #[schema(example = "market_cap")]
    pub sort_by: Option<TokenOrderType>,
    #[schema(example = 1)]
    pub page: i64,
    #[schema(example = 10)]
    pub limit: i64,
}

/// Search token by name, symbol, or token address
#[utoipa::path(
    get,
    path = SearchPath::Search.docs_str(),
    params(
        ("token" = String, Path, description = "Token name, symbol, or address to search for", example = "PUMP"),
        ("sort_by" = Option<String>, Query, description = "Sort order for results (market_cap or creation_time)", example = "market_cap")
    ),
    responses(
        (status = 200, description = "Search tokens successfully", body = SearchResponse),
        (status = 400, description = "Bad request - Invalid sort_by parameter"),
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
    let sort_by = query.sort_by.unwrap_or(TokenOrderType::MarketCap);
    let pagination = PaginationParams {
        page: query.page,
        limit: query.limit,
    };
    let response = order_controller
        .search_order_tokens(&token, sort_by, pagination)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    // 결과를 캐시에 저장
    if let Err(err) = state.redis.set_search_response(&token, &response).await {
        warn!("Failed to cache search response: {}", err);
    }

    Ok(Json(response))
}
