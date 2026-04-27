use axum::{
    Json,
    extract::{Path, State},
};
use tracing::instrument;

use super::path::VaultPath;
use crate::{
    result::{AppError, AppJsonResult},
    services::vault::VaultService,
    state::AppState,
    types::vault::TokenVaultsResponse,
    utils::valid_token_id,
};

/// List vaults that a token routes a portion of its trading fees to.
#[utoipa::path(
    get,
    path = VaultPath::GetTokenVaults.docs_str(),
    params(
        ("token_id" = String, Path, description = "Token address")
    ),
    responses(
        (status = 200, description = "Token vault list fetched successfully", body = TokenVaultsResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Vault"
)]
#[instrument(skip(state))]
pub async fn get_token_vaults(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
) -> AppJsonResult<TokenVaultsResponse> {
    let token_id = valid_token_id(&token_id)
        .ok_or_else(|| AppError::BadRequest("Invalid token id".to_string()))?;

    let service = VaultService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_token_vaults(&token_id).await?;

    Ok(Json(response))
}
