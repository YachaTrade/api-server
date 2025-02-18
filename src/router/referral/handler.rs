use axum::{extract::State, Extension, Json};
use tracing::{error, info, instrument};

use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::referral::{
        CheckRegisterReferralResponse, ExistsReferralCodeResponse, GetReferralCodeResponse, MakeReferralCodeResponse, ReferralController, RegisterReferralRequest, RegisterReferralResponse
    },
};

use super::path::ReferralPath;

// ------------------------Referral Code -------------------------------
/// Check if the account has registered a referral code
#[utoipa::path(
    get,
    path = ReferralPath::CheckReigsterReferral.docs_str(),
    
    responses(
        (status = 200, description = "Successfully checked referral registration", body = CheckRegisterReferralResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Referral",
    security(
        ("session_token" = [])
    )
)]
#[instrument(skip(state))]
pub async fn check_register_referral_code(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<CheckRegisterReferralResponse> {
    let referral_controller = ReferralController::new(state.postgres.clone());
    let response = referral_controller
        .check_register_referral(session_address.clone())
        .await
        .map_err(|err| {
            error!("Failed to check referral registration: session_address: {}, error: {}", session_address, err);
            AppError::InternalError(err.to_string())
        })?;
    Ok(Json(response))
}

/// Register a referral code
#[utoipa::path(
    post,
    path = ReferralPath::RegisterReferral.docs_str(),
    request_body = RegisterReferralRequest,
    responses(
        (status = 200, description = "Successfully registered referral", body = RegisterReferralResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Referral",
    security(
        ("session_token" = [])
    )
)]
#[instrument(skip(state))]
pub async fn register_referral_code(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<RegisterReferralRequest>,
) -> AppJsonResult<RegisterReferralResponse> {
    let referral_controller = ReferralController::new(state.postgres.clone());
    let parent_account_id = referral_controller
        .get_referral_account_id(&payload.parent_referral_code)
        .await
        .map_err(|err| {
            error!("Failed to get referral account ID: session_address: {}, parent_referral_code: {}, error: {}", 
                session_address, payload.parent_referral_code, err);
            AppError::BadRequest(err.to_string())
        })?;
    let response = referral_controller
        .register_referral(&parent_account_id, &session_address)
        .await
        .map_err(|err| {
            error!("Failed to register referral: session_address: {}, parent_account_id: {}, error: {}", 
                session_address, parent_account_id, err);
            AppError::InternalError(err.to_string())
        })?;
    info!("Register Referral Code: session_address :{} response :{:?}", session_address, response);
    Ok(Json(response))
}

// ------------------------Referral Code -------------------------------

/// Check if a referral code exists for the account
#[utoipa::path(
    get,
    path = ReferralPath::GetReferralCode.docs_str(),
    responses(
        (status = 200, description = "Successfully checked referral code existence", body = GetReferralCodeResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Referral",
    security(
        ("session_token" = [])
    )
)]
#[instrument(skip(state))]
pub async fn get_referral_code(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<GetReferralCodeResponse> {
    let referral_controller = ReferralController::new(state.postgres.clone());
    let response = referral_controller
        .get_referral_code(&session_address)
        .await
        .map_err(|err| {
            error!("Failed to check existing referral code: session_address: {}, error: {}", session_address, err);
            AppError::InternalError(err.to_string())
        })?;
    Ok(Json(response))
}

/// Generate a new referral code for the user
#[utoipa::path(
    post,
    path = ReferralPath::MakeReferralCode.docs_str(),
    responses(
        (status = 200, description = "Successfully generated referral code", body = MakeReferralCodeResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Referral",
    security(
        ("session_token" = [])
    )
)]
#[instrument(skip(state))]
pub async fn make_referral_code(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<MakeReferralCodeResponse> {
    let referral_controller = ReferralController::new(state.postgres.clone());
    let is_exists = referral_controller
        .existing_referral_code(&session_address)
        .await
        .map_err(|err| {
            error!("Failed to check existing referral code: session_address: {}, error: {}", session_address, err);
            AppError::InternalError(err.to_string())
        })?;

    if is_exists.exists {
        error!("Referral code already exists for session_address: {}", session_address);
        return Err(AppError::BadRequest(
            "Referral code already exists".to_string(),
        ));
    }

    let response = referral_controller
        .make_referral_code(&session_address)
        .await
        .map_err(|err| {
            error!("Failed to make referral code: session_address: {}, error: {}", session_address, err);
            AppError::InternalError(err.to_string())
        })?;
    Ok(Json(response))
}
