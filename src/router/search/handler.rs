use axum::extract::Path;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::instrument;
use utoipa::ToSchema;

use super::path::Path as SearchPath;

use crate::db::postgres::controller::search::SearchController;
use crate::result::{AppError, AppJsonResult};

use crate::state::AppState;
use crate::types::response::SearchTokenResponse;

#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "tokens": [{
        "token_id": "TEST123",
        "name": "Test Token",
        "symbol": "TEST",
        "image_uri": "https://example.com/test.png",
        "description": "Test token description",
        "created_at": "2024-01-01T00:00:00Z",
        "reply_count": "0",
        "price": "100",
        "user_nickname": "Test User",
        "user_account_id": "user123",
        "user_image_uri": "https://example.com/user.png"
    }]
}))]
pub struct SearchResponse {
    tokens: Vec<SearchTokenResponse>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub token: String,
}

/// Search token by name, symbol, or token address
#[utoipa::path(
    get,
    path = SearchPath::Search.docs_str(),
    params(
        ("token" = String, Path, description = "Token name, symbol, or address to search for", example = "TEST")
    ),
    responses(
        (status = 200, description = "Search tokens successfully", body = SearchResponse),
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
    let tokens = search_contoller.search_order_tokens(&token).await;
    match tokens {
        Ok(tokens) => Ok(Json(SearchResponse { tokens })),
        Err(err) => Err(AppError::InternalError(err.to_string())),
    }
}
