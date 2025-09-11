use axum::{
    Extension, Json,
    extract::{Query, State},
};

use chrono::{self, Timelike};

use tracing::{error, instrument};

use super::path::HypePath;
use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        hype::{
            AmountResponse, HypeController, HypeEpochResponse, HypePointRecordResponse,
            HypePointResponse, HypeRewardAddHistoryResponse, HypeRewardHistoryResponse,
            HypeTokenResponse, HypeVoteHistoryResponse, HypeVoteRequest, HypeVoteResponse,
        },
    },
};

/// Get Hype Token
#[utoipa::path(
    get,
    path = HypePath::GetHype.docs_str(),
    responses(
        (status = 200, description = "Hype Token fetched successfully", body = HypeTokenResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
#[instrument(skip(state))]
pub async fn get_hype_token(State(state): State<AppState>) -> AppJsonResult<HypeTokenResponse> {
    if let Ok(cached_response) = state.redis.get_hype_token_response().await {
        return Ok(Json(cached_response));
    }
    let hype_token_controller = HypeController::new(state.postgres.clone());
    let response = hype_token_controller
        .get_hype_token()
        .await
        .map_err(|err| {
            let error_msg = format!("Failed to get hype token, error: {}", err);
            error!(error_msg);
            AppError::InternalError(error_msg)
        })?;
    if let Err(e) = state.redis.set_hype_token_response(&response).await {
        error!("Failed to set hype token response: {}", e);
    }

    Ok(Json(response))
}

/// Get Hype Point
#[utoipa::path(
    get,
    path = HypePath::GetPoint.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Hype point fetched successfully", body = HypePointResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_point(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<HypePointResponse> {
    if let Ok(cached_response) = state.redis.get_hype_point_response(&session_address).await {
        return Ok(Json(cached_response));
    }
    let hype_controller = HypeController::new(state.postgres.clone());
    let response = hype_controller
        .get_hype_point(&session_address)
        .await
        .map_err(|err| {
            let error_msg = format!("Failed to get hype point, error: {}", err);
            error!(error_msg);
            AppError::InternalError(error_msg)
        })?;
    if let Err(e) = state
        .redis
        .set_hype_point_response(&session_address, &response)
        .await
    {
        error!("Failed to set hype point response: {}", e);
    }

    Ok(Json(response))
}

/// Get Hype Epoch
#[utoipa::path(
    get,
    path = HypePath::GetEpoch.docs_str(),
    responses(
        (status = 200, description = "Hype epoch fetched successfully", body = HypeEpochResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_epoch(State(state): State<AppState>) -> AppJsonResult<HypeEpochResponse> {
    let now = chrono::Utc::now();
    let is_midnight_utc = now.hour() == 0 && now.minute() == 0;

    if !is_midnight_utc {
        if let Ok(cached_response) = state.redis.get_hype_epoch_response().await {
            return Ok(Json(cached_response));
        }
    }
    let hype_token_controller = HypeController::new(state.postgres.clone());
    let response = hype_token_controller
        .get_hype_epoch()
        .await
        .map_err(|err| {
            let error_msg = format!("Failed to get hype epoch, error: {}", err);
            error!(error_msg);
            AppError::InternalError(error_msg)
        })?;
    if let Err(e) = state.redis.set_hype_epoch_response(&response).await {
        error!("Failed to set hype token response: {}", e);
    }

    Ok(Json(response))
}

/// Get Hype Vote History
#[utoipa::path(
    get,
    path = HypePath::GetVoteHistory.docs_str(),
    params(
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page"),
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Hype vote history fetched successfully", body = HypeVoteHistoryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_vote_history(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<HypeVoteHistoryResponse> {
    if let Ok(cached_response) = state
        .redis
        .get_hype_vote_history_response(&session_address, &params)
        .await
    {
        return Ok(Json(cached_response));
    }
    let hype_token_controller = HypeController::new(state.postgres.clone());
    let response = hype_token_controller
        .get_hype_vote_history(&session_address, &params)
        .await
        .map_err(|err| {
            let error_msg = format!("Failed to get hype vote history, error: {}", err);
            error!(error_msg);
            AppError::InternalError(error_msg)
        })?;
    if let Err(e) = state
        .redis
        .set_hype_vote_history_response(&session_address, &params, &response)
        .await
    {
        error!("Failed to set hype vote history response: {}", e);
    }

    Ok(Json(response))
}

/// Get Hype Point History
#[utoipa::path(
    get,
    path = HypePath::GetPointHistory.docs_str(),
    params(
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page"),
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Hype point history fetched successfully", body = HypePointRecordResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_point_history(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<HypePointRecordResponse> {
    let hype_token_controller = HypeController::new(state.postgres.clone());
    let response = hype_token_controller
        .get_hype_point_history(&session_address, &params)
        .await
        .map_err(|err| {
            let error_msg = format!("Failed to get hype point history, error: {}", err);
            error!(error_msg);
            AppError::InternalError(error_msg)
        })?;
    if let Err(e) = state
        .redis
        .set_hype_point_history_response(&session_address, &params, &response)
        .await
    {
        error!("Failed to set hype point history response: {}", e);
    }

    Ok(Json(response))
}

/// Get Hype Reward History
#[utoipa::path(
    get,
    path = HypePath::GetRewardHistory.docs_str(),
    params(
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page"),
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Hype reward history fetched successfully", body = HypeRewardHistoryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_reward_history(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<HypeRewardHistoryResponse> {
    if let Ok(cached_response) = state
        .redis
        .get_hype_reward_history_response(&session_address, &params)
        .await
    {
        return Ok(Json(cached_response));
    }
    let hype_controller = HypeController::new(state.postgres.clone());
    let response = hype_controller
        .get_hype_reward_history(&session_address, &params)
        .await
        .map_err(|err| {
            let error_msg = format!("Failed to get hype reward history, error: {}", err);
            error!(error_msg);
            AppError::InternalError(error_msg)
        })?;
    if let Err(e) = state
        .redis
        .set_hype_reward_history_response(&session_address, &params, &response)
        .await
    {
        error!("Failed to set hype reward history response: {}", e);
    }

    Ok(Json(response))
}

/// Get Hype Reward Add History
#[utoipa::path(
    get,
    path = HypePath::GetRewardAddHistory.docs_str(),
    params(
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page"),
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Hype reward add history fetched successfully", body = HypeRewardAddHistoryResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn get_hype_reward_add_history(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Query(params): Query<PaginationParams>,
) -> AppJsonResult<HypeRewardAddHistoryResponse> {
    if let Ok(cached_response) = state
        .redis
        .get_hype_reward_add_history_response(&session_address, &params)
        .await
    {
        return Ok(Json(cached_response));
    }
    let hype_controller = HypeController::new(state.postgres.clone());
    let response = hype_controller
        .get_hype_reward_add_history(&session_address, &params)
        .await
        .map_err(|err| {
            let error_msg = format!("Failed to get hype reward add history, error: {}", err);
            error!(error_msg);
            AppError::InternalError(error_msg)
        })?;
    if let Err(e) = state
        .redis
        .set_hype_reward_add_history_response(&session_address, &params, &response)
        .await
    {
        error!("Failed to set hype reward add history response: {}", e);
    }

    Ok(Json(response))
}

/// Vote for Hype Token
#[utoipa::path(
    post,
    path = HypePath::Vote.docs_str(),
    request_body = HypeVoteRequest,
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Vote submitted successfully", body = HypeVoteResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
pub async fn vote(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<HypeVoteRequest>,
) -> AppJsonResult<HypeVoteResponse> {
    let hype_controller = HypeController::new(state.postgres.clone());
    let response = hype_controller
        .vote(&session_address, &payload)
        .await
        .map_err(|err| {
            let error_msg = format!("Failed to vote, error: {}", err);
            error!(error_msg);
            AppError::InternalError(error_msg)
        })?;

    Ok(Json(response))
}

/// Get Community Treasury
#[utoipa::path(
    get,
    path = HypePath::GetCommunityTreasury.docs_str(),
    responses(
        (status = 200, description = "Community treasury fetched successfully", body = AmountResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
#[instrument(skip(state))]
pub async fn get_community_treasury(
    State(state): State<AppState>,
) -> AppJsonResult<AmountResponse> {
    // Redis 캐시에서 먼저 확인
    if let Ok(cached_response) = state.redis.get_community_treasury_response().await {
        return Ok(Json(cached_response));
    }

    let hype_controller = HypeController::new(state.postgres.clone());
    let response = hype_controller
        .get_community_treasury()
        .await
        .map_err(|err| {
            let error_msg = format!("Failed to get community treasury, error: {}", err);
            error!(error_msg);
            AppError::InternalError(error_msg)
        })?;

    // Redis 캐시에 저장
    if let Err(e) = state.redis.set_community_treasury_response(&response).await {
        error!("Failed to set community treasury response: {}", e);
    }

    Ok(Json(response))
}

/// Get Total Spend Point
#[utoipa::path(
    get,
    path = HypePath::GetTotalSpendPoint.docs_str(),
    responses(
        (status = 200, description = "Total spend point fetched successfully", body = AmountResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Hype"
)]
#[instrument(skip(state))]
pub async fn get_total_spend_point(State(state): State<AppState>) -> AppJsonResult<AmountResponse> {
    // Redis 캐시에서 먼저 확인
    if let Ok(cached_response) = state.redis.get_total_spend_point_response().await {
        return Ok(Json(cached_response));
    }

    let hype_controller = HypeController::new(state.postgres.clone());
    let response = hype_controller
        .get_total_spend_point()
        .await
        .map_err(|err| {
            let error_msg = format!("Failed to get total spend point, error: {}", err);
            error!(error_msg);
            AppError::InternalError(error_msg)
        })?;

    // Redis 캐시에 저장
    if let Err(e) = state.redis.set_total_spend_point_response(&response).await {
        error!("Failed to set total spend point response: {}", e);
    }

    Ok(Json(response))
}
