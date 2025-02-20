use std::env;

use axum::{extract::State, Extension, Json};
use tracing::{instrument, warn};

use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::admin::{AdminController, DeleteTokenRequest, DeleteTokenResponse},
};

#[instrument(skip(state))]
pub async fn delete_token(
    State(state): State<AppState>,
    Json(payload): Json<DeleteTokenRequest>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<DeleteTokenResponse> {
    let DeleteTokenRequest { token_id, password } = payload;
    let env_passwrod = env::var("ADMIN_PASSWORD").expect("ADMIN_PASSWORD must be set");

    if password != env_passwrod {
        return Err(AppError::Unauthorized("Unauthorized".to_string()));
    }

    let env_admin_address = env::var("ADMIN_ADDRESS").expect("ADMIN_ADDRESS must be set");
    if session_address != env_admin_address {
        return Err(AppError::Unauthorized("Unauthorized".to_string()));
    }

    let admin_controller = AdminController::new(state.postgres.clone());
    let response = admin_controller
        .delete_token(&token_id)
        .await
        .map_err(|err| {
            warn!("delete token Error {:?}", err);
            AppError::BadRequest(err.to_string())
        })?;

    //check session address ==
    Ok(Json(response))
}
