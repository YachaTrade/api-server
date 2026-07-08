use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    response::Redirect,
};
use tracing::instrument;

use super::path::XVerificationPath;
use crate::{
    config::{X_OAUTH_REDIRECT_FAILURE_URL, X_OAUTH_REDIRECT_SUCCESS_URL},
    result::{AppError, AppJsonResult},
    services::rate_limiter::{
        RateLimitResult, X_FOLLOWED_BY_RATE_LIMIT, X_OAUTH_LOGIN_RATE_LIMIT, X_RESERVE_RATE_LIMIT,
        check_and_increment,
    },
    services::x_verification::XVerificationService,
    state::AppState,
    types::x_verification::{
        FinalizeRequest, FinalizeResponse, FollowedByRequest, FollowedByResponse, LogoutResponse,
        OAuthCallbackQuery, OAuthLoginResponse, ReserveRequest, ReserveResponse, StatusResponse,
        validate_handle,
    },
    utils::valid_account_id,
};

fn service(state: &AppState) -> XVerificationService {
    XVerificationService::new(state.postgres.clone(), state.redis.clone())
}

/// Append a fixed error code to the (server-controlled) failure URL.
fn failure_redirect(code: &str) -> Redirect {
    let sep = if X_OAUTH_REDIRECT_FAILURE_URL.contains('?') {
        '&'
    } else {
        '?'
    };
    Redirect::to(&format!(
        "{}{}x_verify_error={}",
        *X_OAUTH_REDIRECT_FAILURE_URL, sep, code
    ))
}

/// POST /x/oauth/login — start PKCE OAuth. Returns the X authorize URL.
#[utoipa::path(
    post, path = XVerificationPath::OauthLogin.docs_str(),
    responses((status = 200, body = OAuthLoginResponse)),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address))]
pub async fn oauth_login(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<OAuthLoginResponse> {
    rate_limit(
        &state,
        &format!("x_login:{session_address}"),
        X_OAUTH_LOGIN_RATE_LIMIT,
    )
    .await?;
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    let resp = service(&state).start_login(&account_id).await?;
    Ok(Json(resp))
}

/// POST /x/oauth/logout — clear the current session's pending X-OAuth login.
/// Idempotent: succeeds whether or not the session is currently logged in.
#[utoipa::path(
    post, path = XVerificationPath::OauthLogout.docs_str(),
    responses((status = 200, body = LogoutResponse)),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address))]
pub async fn oauth_logout(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<LogoutResponse> {
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    service(&state).logout(&account_id).await?;
    Ok(Json(LogoutResponse { ok: true }))
}

/// GET /x/oauth/callback — PUBLIC (X redirects here). Always redirects the
/// browser to a FIXED server env URL (never client-supplied).
#[utoipa::path(
    get, path = XVerificationPath::OauthCallback.docs_str(),
    responses((status = 302, description = "Redirect to fixed front-end URL")),
    tag = "XVerification"
)]
#[instrument(skip(state))]
pub async fn oauth_callback(
    State(state): State<AppState>,
    Query(q): Query<OAuthCallbackQuery>,
) -> Redirect {
    if q.error.is_some() {
        return failure_redirect("denied");
    }
    let (code, st) = match (q.code, q.state) {
        (Some(c), Some(s)) if !c.is_empty() && !s.is_empty() => (c, s),
        _ => return failure_redirect("bad_request"),
    };
    match service(&state).complete_callback(&code, &st).await {
        Ok(_) => Redirect::to(&X_OAUTH_REDIRECT_SUCCESS_URL),
        Err(AppError::Gone(_)) => failure_redirect("state_expired"),
        Err(_) => failure_redirect("x_error"),
    }
}

/// POST /x/followed-by — check + record that a handle follows the creator.
#[utoipa::path(
    post, path = XVerificationPath::FollowedBy.docs_str(),
    request_body = FollowedByRequest,
    responses(
        (status = 200, body = FollowedByResponse),
        (status = 400, description = "invalid_handle | insufficient_followers")
    ),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address))]
pub async fn add_followed_by(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<FollowedByRequest>,
) -> AppJsonResult<FollowedByResponse> {
    let handle = payload.handle.trim().trim_start_matches('@').to_string();
    if !validate_handle(&handle) {
        return Err(AppError::BadRequest("invalid_handle".into()));
    }
    rate_limit(
        &state,
        &format!("x_followed_by:{session_address}"),
        X_FOLLOWED_BY_RATE_LIMIT,
    )
    .await?;
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    let resp = service(&state)
        .add_followed_by(&account_id, &handle)
        .await?;
    Ok(Json(resp))
}

/// DELETE /x/followed-by/:handle — drop a handle from the pending list.
#[utoipa::path(
    delete, path = XVerificationPath::FollowedByDelete.docs_str(),
    params(("handle" = String, Path, description = "X handle to remove")),
    responses((status = 200, body = StatusResponse)),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address))]
pub async fn delete_followed_by(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(handle): Path<String>,
) -> AppJsonResult<StatusResponse> {
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    let resp = service(&state)
        .remove_followed_by(&account_id, &handle)
        .await?;
    Ok(Json(resp))
}

/// GET /x/verification/status — current verification-progress signals (no creator handle).
#[utoipa::path(
    get, path = XVerificationPath::Status.docs_str(),
    responses((status = 200, body = StatusResponse)),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address))]
pub async fn get_status(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<StatusResponse> {
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    let resp = service(&state).get_status(&account_id).await?;
    Ok(Json(resp))
}

/// POST /x/verification/reserve — pre-deploy first-writer reservation (C1 fix).
/// `skip(payload)` on the span so the salt is never logged.
#[utoipa::path(
    post, path = XVerificationPath::Reserve.docs_str(),
    request_body = ReserveRequest,
    responses(
        (status = 200, body = ReserveResponse),
        (status = 403, description = "creator_mismatch"),
        (status = 409, description = "already_deployed | token_already_reserved"),
        (status = 503, description = "onchain_check_unavailable")
    ),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn reserve(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<ReserveRequest>,
) -> AppJsonResult<ReserveResponse> {
    rate_limit(
        &state,
        &format!("x_reserve:{session_address}"),
        X_RESERVE_RATE_LIMIT,
    )
    .await?;
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    service(&state).reserve(&payload, &account_id).await?;
    Ok(Json(ReserveResponse { ok: true }))
}

/// POST /x/verification/finalize — reservation-only (design §7.1-B): verify
/// the calling session owns a prior `reserve()` for this `token_id`, then
/// persist the pending X-verification signals. The CREATE2/creator checks
/// now live in `reserve`, not here.
#[utoipa::path(
    post, path = XVerificationPath::Finalize.docs_str(),
    request_body = FinalizeRequest,
    responses(
        (status = 200, body = FinalizeResponse),
        (status = 403, description = "not_reserved"),
        (status = 409, description = "x_account_mismatch"),
        (status = 410, description = "verification_expired")
    ),
    tag = "XVerification"
)]
#[instrument(skip(state, session_address))]
pub async fn finalize(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<FinalizeRequest>,
) -> AppJsonResult<FinalizeResponse> {
    let account_id = valid_account_id(&session_address)
        .ok_or_else(|| AppError::BadRequest("invalid session address".into()))?;
    service(&state).finalize(&payload, &account_id).await?;
    Ok(Json(FinalizeResponse { ok: true }))
}

async fn rate_limit(state: &AppState, id: &str, limit: u64) -> Result<(), AppError> {
    match check_and_increment(&state.redis, id, limit).await? {
        RateLimitResult::Exceeded { retry_after, .. } => {
            Err(AppError::TooManyRequests { retry_after })
        }
        RateLimitResult::Allowed { .. } => Ok(()),
    }
}
