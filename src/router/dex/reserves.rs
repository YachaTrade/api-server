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
    types::dex::reserves::{ReservesQuery, ReservesResponse},
    utils::valid_account_id,
};

/// Current reserves of a V2 pool — a backend-served drop-in for an on-chain
/// `lpPair.getReserves()` call (reserves come from the indexer-maintained `pool`
/// row, so no RPC). The client orients in/out by `token0`/`token1` and applies
/// the swap math. Returns 404 when the pool doesn't exist.
#[utoipa::path(
    get,
    path = DexPath::GetReserves.docs_str(),
    params(ReservesQuery),
    responses(
        (status = 200, description = "Reserves fetched successfully", body = ReservesResponse),
        (status = 400, description = "Invalid pool id"),
        (status = 404, description = "Pool not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Dex"
)]
#[instrument(skip(state))]
pub async fn get_reserves(
    State(state): State<AppState>,
    Query(params): Query<ReservesQuery>,
) -> AppJsonResult<ReservesResponse> {
    let pool_id = valid_account_id(&params.pool_id)
        .ok_or_else(|| AppError::BadRequest("Invalid pool id".to_string()))?;

    let service = DexService::new(state.postgres.clone());
    let response = service
        .get_reserves(&pool_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Pool not found: {}", pool_id)))?;

    Ok(Json(response))
}
