use axum::{Json, extract::State};
use tracing::instrument;

use super::path::QuoteTokenPath;
use crate::{
    result::AppJsonResult, services::quote_token::QuoteTokenService, state::AppState,
    types::quote_token::QuoteTokensResponse,
};

/// List all registered quote tokens.
#[utoipa::path(
    get,
    path = QuoteTokenPath::List.docs_str(),
    responses(
        (status = 200, description = "Quote tokens fetched successfully", body = QuoteTokensResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "QuoteToken"
)]
#[instrument(skip(state))]
pub async fn list_quote_tokens(
    State(state): State<AppState>,
) -> AppJsonResult<QuoteTokensResponse> {
    let service = QuoteTokenService::new(state.postgres.clone(), state.redis.clone());
    let response = service.list().await?;
    Ok(Json(response))
}
