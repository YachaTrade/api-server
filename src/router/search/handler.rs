use axum::extract::{Path, Query};
use axum::{extract::State, Json};
use serde::Deserialize;

use tracing::{error, info, instrument, warn};

use utoipa::{schema, ToSchema};

use crate::result::{AppError, AppJsonResult};
use crate::state::AppState;
use crate::types::common::pagination::PaginationParams;
use crate::types::token::order::{OrderController, SearchResponse, TokenOrderType};

use super::path::SearchPath;

#[derive(Debug, Deserialize, ToSchema)]
#[schema(example = json!({
    "order_type": "latest_trade",
    "page": 1,
    "limit": 10
}))]
pub struct SearchTokenQuery {
    #[schema(example = "latest_trade")]
    pub order_type: Option<TokenOrderType>,
    #[schema(example = 1)]
    pub page: Option<i64>,
    #[schema(example = 10)]
    pub limit: Option<i64>,
}

/// Search token by name, symbol, or token address
#[utoipa::path(
    get,
    path = SearchPath::Search.docs_str(),
    params(
        ("token" = String, Path, description = "Token name, symbol, or address to search for", example = "PUMP"),
        ("order_type" = Option<TokenOrderType>, Query, description = "Order type for results (market_cap, creation_time, latest_trade)", example = "market_cap"),
        ("page" = Option<i64>, Query, description = "Number of results to return", example = 1),
        ("limit" = Option<i64>, Query, description = "Number of results to skip", example = 10)
    ),
    responses(
        (status = 200, description = "Search tokens successfully", body = SearchResponse),
        (status = 400, description = "Bad request - Invalid parameters"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Search"
)]
#[instrument(skip_all)]
pub async fn search_token(
    State(state): State<AppState>,
    Path(token): Path<String>,
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
    let sort_by = query.order_type.unwrap_or(TokenOrderType::MarketCap);
    let pagination = PaginationParams {
        page: query.page.unwrap_or(1),
        limit: query.limit.unwrap_or(10),
    };

    let response = order_controller
        .search_order_tokens(&token, sort_by, pagination)
        .await
        .map_err(|err| {
            error!("Failed to search token: token: {}, error: {}", token, err);
            AppError::InternalError(err.to_string())
        })?;
    info!("Search Token: token: {}, response: {:?}", token, response);

    // 결과를 캐시에 저장
    if let Err(err) = state.redis.set_search_response(&token, &response).await {
        warn!("Failed to cache search response: {}", err);
    }

    Ok(Json(response))
}
