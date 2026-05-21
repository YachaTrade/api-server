use axum::{
    Json,
    extract::{Path, State},
};
use tracing::instrument;

use super::path::DexPath;
use crate::{
    result::AppJsonResult, services::dex::DexService, state::AppState,
    types::dex::position::LpPositionsResponse, utils::valid_account_id,
};

/// List a wallet's open V2 LP positions (one row per pool with `balance > 0`).
#[utoipa::path(
    get,
    path = DexPath::GetPositions.docs_str(),
    params(
        ("account_id" = String, Path, description = "Wallet address (EIP-55 checksum)")
    ),
    responses(
        (status = 200, description = "Open LP positions for the wallet", body = LpPositionsResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Dex"
)]
#[instrument(skip(state))]
pub async fn get_positions(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> AppJsonResult<LpPositionsResponse> {
    let account_id = valid_account_id(&account_id)
        .ok_or_else(|| crate::result::AppError::BadRequest("Invalid account id".to_string()))?;

    let service = DexService::new(state.postgres.clone());
    let response = service.get_positions(&account_id).await?;

    Ok(Json(response))
}
