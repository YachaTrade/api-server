use axum::extract::Path;
use axum::{extract::State, Json};
use serde::Serialize;
use tracing::instrument;
use utoipa::ToSchema;

use super::path::Path as SearchPath;

use crate::db::postgres::controller::search::SearchController;
use crate::result::AppJsonResult;

use crate::state::AppState;
use crate::types::response::SearchTokenResponse;

#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResponse {
    #[schema(example = "name or symbol or token address")]
    tokens: Vec<SearchTokenResponse>,
}
/// Search token by name, symbol, or token address
#[utoipa::path(
    get,
    path = SearchPath::Search.docs_str(),
    params(
        ("token address or symbol or name" = String, Path, description = "Token to search for")
    ),
    responses(
        (status = 200, description = "Nonce generated successfully", body = SearchResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Search Token"
)]
#[instrument(skip(token, state))]
pub async fn search_token(
    Path(token): Path<String>,
    State(state): State<AppState>,
) -> AppJsonResult<SearchResponse> {
    let search_contoller = SearchController::new(state.postgres.clone());
    let tokens = search_contoller.search_order_tokens(&token).await?;
    Ok(Json(SearchResponse { tokens }))
}
