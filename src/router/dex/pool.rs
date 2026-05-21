use axum::{
    Json,
    extract::{Path, State},
};
use tracing::instrument;

use super::path::DexPath;
use crate::{
    result::{AppError, AppJsonResult},
    services::dex::DexService,
    state::AppState,
    types::dex::pool::PoolDetailResponse,
    utils::valid_account_id,
};

/// Pool detail for the Deposit/Withdraw UI: reserves, TVL, total LP supply,
/// 7d APR, fee_config breakdown. Returns 404 when the pool doesn't exist.
#[utoipa::path(
    get,
    path = DexPath::GetPool.docs_str(),
    params(
        ("pool_id" = String, Path, description = "Pool (pair) address (EIP-55 checksum)")
    ),
    responses(
        (status = 200, description = "Pool detail fetched successfully", body = PoolDetailResponse),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Pool not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Dex"
)]
#[instrument(skip(state))]
pub async fn get_pool(
    State(state): State<AppState>,
    Path(pool_id): Path<String>,
) -> AppJsonResult<PoolDetailResponse> {
    let pool_id = valid_account_id(&pool_id)
        .ok_or_else(|| AppError::BadRequest("Invalid pool id".to_string()))?;

    let service = DexService::new(state.postgres.clone());
    let response = service
        .get_pool_detail(&pool_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Pool not found: {}", pool_id)))?;

    Ok(Json(response))
}
