use axum::{
    Extension, Json,
    extract::{Query, State},
};
use tracing::instrument;

use super::path::DexPath;
use crate::{
    result::AppJsonResult,
    services::dex::DexService,
    state::AppState,
    types::dex::{search::DexSearchQuery, tokens::DexTokenListResponse},
};

/// Session-authenticated DEX token search. Returns matching tokens with the
/// logged-in user's balance attached. Marketcap-desc sorted. Returns 401 when
/// session cookie is missing or invalid.
#[utoipa::path(
    get,
    path = DexPath::SearchTokens.docs_str(),
    params(DexSearchQuery),
    responses(
        (status = 200, description = "Token search results", body = DexTokenListResponse),
        (status = 401, description = "Missing or invalid session"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Dex"
)]
#[instrument(skip(state, session_address))]
pub async fn search_tokens(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(params): Query<DexSearchQuery>,
) -> AppJsonResult<DexTokenListResponse> {
    let service = DexService::new(state.postgres.clone());
    let response = service.search_tokens(&params, &session_address).await?;
    Ok(Json(response))
}
