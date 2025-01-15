use axum::extract::{Path, Query};
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use tracing::{instrument, warn};
use utoipa::ToSchema;

use crate::db::postgres::controller::order::OrderController;
use crate::db::postgres::controller::token::TokenController;
use crate::result::{AppError, AppJsonResult};
use crate::state::AppState;
use crate::types::order::TokenOrderType;
use crate::types::response::SearchResponse;

use super::path::SearchPath;
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
#[derive(Deserialize)]
pub struct SearchTokenQuery {
    #[schema(example = "market_cap")]
    pub sort_by: Option<TokenOrderType>,
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
    tag = "Search Token"
)]
#[instrument(skip(state))]
pub async fn search_token(
    Path(token): Path<String>,
    State(state): State<AppState>,
    Query(query): Query<SearchTokenQuery>,
) -> AppJsonResult<SearchResponse> {
    // 캐시된 결과 확인
    if let Ok(cached_response) = state.redis.get_search_response(&token).await {
        return Ok(Json(cached_response));
    }

    let order_controller = OrderController::new(state.postgres.clone());
    let sort_by = query.sort_by.unwrap_or(TokenOrderType::MarketCap);
    let tokens = order_controller
        .search_order_tokens(&token, sort_by)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    let response = SearchResponse { tokens };

    // 결과를 캐시에 저장
    if let Err(err) = state.redis.set_search_response(&token, &response).await {
        warn!("Failed to cache search response: {}", err);
    }

    Ok(Json(response))
}
