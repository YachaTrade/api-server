use axum::{
    Json,
    extract::{Path, State},
};

use tracing::instrument;

use crate::{
    controllers::chester::ChesterController,
    result::AppJsonResult,
    state::AppState,
    types::chester::{
        ChesterInfoResponse, ChesterRewardsResponse, ChesterVolumeResponse,
    },
};

use super::path::ChesterPath;

/// Get cumulative USD trading volume for an account in the active round
#[utoipa::path(
    get,
    path = ChesterPath::Volume.docs_str(),
    params(
        ("account_id" = String, Path, description = "Account address")
    ),
    responses(
        (status = 200, description = "Account trading volume", body = ChesterVolumeResponse)
    ),
    tag = "Chester"
)]
#[instrument(skip(state))]
pub async fn get_volume(
    State(state): State<AppState>,
    Path(account_id): Path<String>,
) -> AppJsonResult<ChesterVolumeResponse> {
    let controller = ChesterController::new(state.postgres.clone());
    let response = controller.get_volume(&account_id).await?;

    Ok(Json(response))
}

/// Get current active round information
#[utoipa::path(
    get,
    path = ChesterPath::Round.docs_str(),
    responses(
        (status = 200, description = "Current round information", body = ChesterInfoResponse)
    ),
    tag = "Chester"
)]
#[instrument(skip(state))]
pub async fn get_round(
    State(state): State<AppState>,
) -> AppJsonResult<Option<ChesterInfoResponse>> {
    let controller = ChesterController::new(state.postgres.clone());
    let response = controller.get_round().await?;

    Ok(Json(response))
}

/// Get reward items with USD values for the active round
#[utoipa::path(
    get,
    path = ChesterPath::Rewards.docs_str(),
    responses(
        (status = 200, description = "Round rewards with USD values", body = ChesterRewardsResponse)
    ),
    tag = "Chester"
)]
#[instrument(skip(state))]
pub async fn get_rewards(
    State(state): State<AppState>,
) -> AppJsonResult<ChesterRewardsResponse> {
    let controller = ChesterController::new(state.postgres.clone());
    let response = controller.get_rewards().await?;

    Ok(Json(response))
}
