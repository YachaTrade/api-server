use axum::{
    Json,
    extract::{Query, State},
};
use tracing::instrument;

use super::path::DexPath;
use crate::{
    result::{AppError, AppJsonResult},
    services::dex::DexService,
    state::AppState,
    types::dex::tokens::{DexTokenListQuery, DexTokenListResponse},
    utils::valid_account_id,
};

/// V2 DEX-tradeable token list with optional per-account balance, search, and
/// pagination. Tokens are filtered to those appearing as `token0` or `token1`
/// in at least one `pool` row.
#[utoipa::path(
    get,
    path = DexPath::GetTokens.docs_str(),
    params(DexTokenListQuery),
    responses(
        (status = 200, description = "Token list fetched successfully", body = DexTokenListResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Dex"
)]
#[instrument(skip(state))]
pub async fn get_tokens(
    State(state): State<AppState>,
    Query(mut params): Query<DexTokenListQuery>,
) -> AppJsonResult<DexTokenListResponse> {
    // Normalize account to EIP-55 checksum if provided.
    if let Some(raw) = params.account.as_deref() {
        params.account = Some(
            valid_account_id(raw)
                .ok_or_else(|| AppError::BadRequest("Invalid account id".to_string()))?,
        );
    }

    let service = DexService::new(state.postgres.clone());
    let response = service.get_tokens(&params).await?;

    Ok(Json(response))
}
