use axum::extract::{Path, Query};
use axum::{extract::State, Json};

use tracing::{debug, error, info, instrument, warn};

use crate::result::{AppError, AppJsonResult};
use crate::state::AppState;
use crate::types::common::pagination::PaginationParams;
use crate::types::search::{SearchController, SearchResponse};

use super::path::SearchPath;

/// Search account , token
#[utoipa::path(
    get,
    path = SearchPath::Search.docs_str(),
    params(
        ("name" = String, Path, description = "Token name, symbol, or address to search for", example = "PUMP"),
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
#[instrument(skip(state))]
pub async fn search(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(pagination): Query<PaginationParams>,
) -> AppJsonResult<SearchResponse> {
    if name.is_empty() {
        warn!("Empty name search query");
        return Err(AppError::BadRequest("Empty name search query".to_string()));
    }

    if name == "0x" {
        warn!("Invalid name search query");
        return Err(AppError::BadRequest(
            "0x is invalid search path".to_string(),
        ));
    }

    // 캐시된 결과에서 페이지네이션
    if let Ok(Some(cached_response)) = state.redis.get_search_response(&name, pagination).await {
        debug!("Cache hit for search query: {}", name);
        return Ok(Json(cached_response));
    }

    // DB에서 검색 수행
    let response = SearchController::new(state.postgres.clone())
        .search(&name)
        .await
        .map_err(|err| {
            error!("Failed to search : {}, error: {}", name, err);
            AppError::InternalError(err.to_string())
        })?;

    // 결과를 캐시에 저장
    if let Err(err) = state.redis.set_search_response(&name, &response).await {
        warn!("Failed to cache search response: {}", err);
    }

    Ok(Json(response))
}
