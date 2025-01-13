use axum::extract::{Path, Query};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::instrument;
use utoipa::ToSchema;

use super::path::Path as SearchPath;

use crate::db::postgres::controller::search::{SearchController, TokenSortBy};
use crate::result::{AppError, AppJsonResult};

use crate::state::AppState;
use crate::types::response::SearchTokenResponse;

#[derive(Debug, Serialize, ToSchema)]
#[schema(example = json!({
    "tokens": [{
        "token_info": {
            "token_id": "PUMP_TOKEN_001",
            "name": "NAD Fun Token",
            "symbol": "PUMP",
            "image_uri": "token_image_uri",
            "description": "this is a description.",
            "reply_count": "128",
            "price": "1250000",
            "reserve_token": "1000000",
            "created_at": 1703400000,
            "market_type": "DEX",
            "is_king": true,
            "score": 128.0
        },
        "account_info": {
            "account_id": "0xaasdfasdfasdfas",
            "nickname": "master",
            "image_uri": "https://storage.googleapis.com/nads-profiles/user_01.png"
        }
    }]
}))]
pub struct SearchResponse {
    tokens: Vec<SearchTokenResponse>,
}

#[derive(Debug, Deserialize)]
pub struct SearchTokenQuery {
    pub sort_by: Option<TokenSortBy>,
}
/// Search token by name, symbol, or token address
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
    tag = "Search Token"
)]
#[instrument(skip(token, state))]
pub async fn search_token(
    Path(token): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<SearchTokenQuery>,
) -> AppJsonResult<SearchResponse> {
    let search_contoller = SearchController::new(state.postgres.clone());
    let sort_by = query.sort_by.unwrap_or(TokenSortBy::MarketCap);
    let tokens = search_contoller
        .search_order_tokens(&token, sort_by)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;
    Ok(Json(SearchResponse { tokens }))
}
