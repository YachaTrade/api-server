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

/// DEPRECATED — use `GET /dex/tokens?q=` instead. Session-authenticated token
/// search; delegates to the unified token list.
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
#[deprecated(note = "use GET /dex/tokens?q= instead")]
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
