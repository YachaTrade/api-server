use axum::extract::{Path, Query};
use axum::{Json, extract::State};

use tracing::{instrument, warn};

use crate::result::{AppError, AppJsonResult};
use crate::services::search::SearchService;
use crate::state::AppState;
use crate::types::common::pagination::PaginationParams;
use crate::types::search::SearchResponse;

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

    if name.len() > 100 {
        warn!("Search query too long: {} chars", name.len());
        return Err(AppError::BadRequest("Search query too long (max 100 characters)".to_string()));
    }

    if name == "0x" {
        warn!("Invalid name search query");
        return Err(AppError::BadRequest(
            "0x is invalid search path".to_string(),
        ));
    }

    let service = SearchService::new(state.postgres.clone(), state.redis.clone());
    let response = service.search(&name, pagination).await?;
    Ok(Json(response))
}
