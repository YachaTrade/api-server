use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use tracing::{error, instrument};

use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        campaign::{
            active::{ActiveUserController, ActiveUserResponse},
            point::{
                AccountPointResponse, MissionCompleteRequest, MissionCompleteResponse,
                MissionCompletedResponse, PointController, TopPointResponse,
            },
        },
        common::pagination::PaginationParams,
    },
    utils::valid_evm_address,
};

use super::path::{ActiveUserPath, PointPath};
//------------------------------------------------------------Active User------------------------------------------------------------//
/// Check if a user is active
#[utoipa::path(
    get,
    path = ActiveUserPath::CheckActiveUser.docs_str(),
    params(
        ("wallet_address" = String, Path, description = "Ethereum wallet address to check")
    ),
    responses(
        (status = 200, description = "Successfully retrieved active user status", body = ActiveUserResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Campaign"
)]
#[instrument(skip(state))]
pub async fn check_active_user(
    Path(wallet_address): Path<String>,
    State(state): State<AppState>,
) -> AppJsonResult<ActiveUserResponse> {
    if !valid_evm_address(&wallet_address) {
        return Err(AppError::BadRequest("Invalid wallet address".to_string()));
    }
    let response = ActiveUserController::new(state.postgres.clone())
        .check_active_user(&wallet_address)
        .await
        .map_err(|err| {
            error!(
                "Failed to check active user: wallet_address: {}, error: {}",
                wallet_address, err
            );
            AppError::InternalError(err.to_string())
        })?;
    Ok(Json(response))
}

//------------------------------------------------------------Score------------------------------------------------------------//
/// Get top point rankings with pagination
#[utoipa::path(
    get,
    path = PointPath::GetTop.docs_str(),
    params(
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items limit")
    ),
    responses(
        (status = 200, description = "Successfully retrieved top point rankings", body = TopPointResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Campaign"
)]
#[instrument(skip(state))]
pub async fn get_top_point(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<TopPointResponse> {
    if let Ok(cached_response) = state.trade_redis.get_top_point_response(&params).await {
        return Ok(Json(cached_response));
    }
    let response = match PointController::new(state.postgres.clone())
        .get_top_point(&params)
        .await
    {
        Ok(res) => res,
        Err(err) => {
            error!("Failed to get top point: {}", err);
            return Err(AppError::InternalError(err.to_string()));
        }
    };
    if let Err(err) = state
        .trade_redis
        .set_top_point_response(&params, &response)
        .await
    {
        error!("Failed to set top point response: {:?}", err);
    }
    Ok(Json(response))
}
///Get account point by account id
#[utoipa::path(
    get,
    path = PointPath::GetAccountPoint.docs_str(),
    responses(
        (status = 200, description = "Successfully retrieved account point", body = AccountPointResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Campaign",
    security(
        ("session_token" = [])
    )
)]
#[instrument(skip(state))]
pub async fn get_point_by_account_id(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<AccountPointResponse> {
    if let Ok(cached_response) = state
        .trade_redis
        .get_point_by_account_id(&session_address)
        .await
    {
        return Ok(Json(cached_response));
    }
    let response = match PointController::new(state.postgres.clone())
        .get_account_point_rank(&session_address)
        .await
    {
        Ok(res) => res,
        Err(err) => {
            error!("Failed to get account point rank: {}", err);
            return Err(AppError::InternalError(err.to_string()));
        }
    };
    if let Err(err) = state
        .trade_redis
        .set_point_by_account_id(&session_address, &response)
        .await
    {
        error!("Failed to set point by account id: {:?}", err);
    }
    Ok(Json(response))
}

///Complete mission
#[utoipa::path(
    get,
    path = PointPath::CompleteMission.docs_str(),
    request_body = MissionCompleteRequest,
    responses(
        (status = 200, description = "Successfully retrieved account point", body = MissionCompleteResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Campaign",
    security(
        ("session_token" = [])
    )
)]
#[instrument(skip(state))]
pub async fn complete_mission(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<MissionCompleteRequest>,
) -> AppJsonResult<MissionCompleteResponse> {
    let MissionCompleteRequest { mission_type } = payload;
    if let Ok(cached_response) = state
        .trade_redis
        .get_complete_mission(&session_address)
        .await
    {
        return Ok(Json(cached_response));
    }
    let response = match PointController::new(state.postgres.clone())
        .add_point_by_mission(&session_address, mission_type)
        .await
    {
        Ok(res) => res,
        Err(err) => {
            error!("Failed to add point by mission: {}", err);
            return Err(AppError::BadRequest(err.to_string()));
        }
    };

    if let Err(err) = state
        .trade_redis
        .set_complete_mission(&session_address, &response)
        .await
    {
        error!("Failed to set complete mission: {:?}", err);
    }
    Ok(Json(response))
}

///Get completed missions
#[utoipa::path(
    post,
    path = PointPath::GetCompletedMissions.docs_str(),
    responses(
        (status = 200, description = "Successfully retrieved account point", body = MissionCompletedResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Campaign",
    security(
        ("session_token" = [])
    )
)]
#[instrument(skip(state))]
pub async fn get_completed_missions(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<MissionCompletedResponse> {
    if let Ok(cached_response) = state
        .trade_redis
        .get_completed_missions(&session_address)
        .await
    {
        return Ok(Json(cached_response));
    }
    let response = match PointController::new(state.postgres.clone())
        .get_completed_missions(&session_address)
        .await
    {
        Ok(res) => res,
        Err(e) => {
            error!("Failed to get completed missions: {:?}", e);
            return Err(AppError::InternalError(e.to_string()));
        }
    };

    if let Err(err) = state
        .trade_redis
        .set_completed_missions(&session_address, &response)
        .await
    {
        error!("Failed to set completed missions: {:?}", err);
    }
    Ok(Json(response))
}
