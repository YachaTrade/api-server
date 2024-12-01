use axum::{
    extract::{Multipart, State},
    Extension, Json, 
};
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};
use utoipa::ToSchema;

use crate::{
    db::postgres::{controller::{account::AccountController, account_like::AccountLikeController}, model::Account}, result::{AppError, AppJsonResult}, state::AppState

};

use super::path::Path;

#[derive(Debug, Deserialize, ToSchema)]

pub struct UpdateAccountRequest {
    #[schema(example = json!("Your Nickname" ), nullable)]
    #[serde(rename = "nickName")]
    pub nick_name: Option<String>,
    #[serde(rename = "bio")]
    pub bio:Option<String>
}


#[derive(ToSchema)]
pub struct UpdateAccountFormData {
    #[schema(example = json!({
        "nick_name": "user nickname",
    }))]
    pub data: UpdateAccountRequest, // JSON string

    #[schema(format = "binary")]
    pub image: Option<Vec<u8>>,
}



#[derive(Debug, Serialize, ToSchema)]
pub struct AccountResponse {
    #[schema(example = json!({
        "address": "address",
        "nickName": "Nickname",
        "bio":"bio",
        "image": "image",
        "like_count": 0,
        "follower_count": 0,
        "following_count": 0,
    }))]
    account: Account,
}

/// Update account profile
#[utoipa::path(
    patch,
    path = Path::UpdateAccount.as_str(),
    request_body(
        content = UpdateAccountFormData,
        content_type = "multipart/form-data",
        description = "Account update nickname and image",
        example = json!({ 
            "data": {
                "nickName": "Your Nickname"
            },
            "image": "[binary]"
        })
    ),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Account updated successfully", body = AccountResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag="Account"
)]

#[instrument(skip(state, session_address, multipart))]
pub async fn update_account(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    mut multipart: Multipart,
) -> AppJsonResult<AccountResponse> {
    info!("Updating account for user: {}", session_address);
    
    let mut form_data = None;
    let mut image_data = None;
    let mut content_type = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "data" {
            let data: String = field
                .text()
                .await
                .map_err(|err| AppError::BadRequest(err.to_string()))?;
            form_data = Some(
                serde_json::from_str::<UpdateAccountRequest>(&data)
                    .map_err(|err| AppError::BadRequest(err.to_string()))?,
            );
        } else if name == "image" {
            content_type = field.content_type().map(|ct| ct.to_string());
            image_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|err| AppError::BadRequest(err.to_string()))?,
            );
        }
    }

    let form_data = form_data.ok_or_else(|| AppError::BadRequest("Missing account data".into()))?;

    if form_data.nick_name.is_none() &&  form_data.bio.is_none() && image_data.is_none(){
        return Err(AppError::BadRequest(
            "At least one of nickName or image must be provided".into(),
        ));
    }

    let nick_name = form_data.nick_name.and_then(|name| {
        let trimmed = name.replace(" ", "");
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });
    let bio =  form_data.bio.and_then(|name| {
        let trimmed = name.replace(" ", "");
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    });

    let image_uri = if let Some(image_data) = image_data {
        let r2client = state.r2client.clone();
        let content_type = content_type.unwrap_or("image/jpg".to_string());
        let image_uri = r2client
            .upload_profile_image_file(&session_address, image_data, &content_type)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;
        Some(image_uri)
    } else {
        None
    };

    let account_controller = AccountController::new(state.postgres.clone());
    let updated_account = account_controller
        .update_account(&session_address, image_uri, nick_name,bio)
        .await
        .map_err(|err| {
            info!("update Account Error {:?}", err);
            AppError::BadRequest(err.to_string())
        })?;

    Ok(Json(AccountResponse {
        account: updated_account,
    }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AddLikeRequest {
    #[schema(example = "target_address")]
    pub target_address: String,
}

/// Add account like
#[utoipa::path(
    patch,
    path = Path::AddAccountLike.as_str(),
    request_body = AddLikeRequest,
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Account like successfully", body = AccountResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag="Account"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn add_account_like( 
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<AddLikeRequest>
) -> AppJsonResult<AccountResponse> {
    info!("Adding like for user: {}", session_address);

    let target_address = payload.target_address;

    let account_like_controller = AccountLikeController::new(state.postgres.clone());

    let updated_account = account_like_controller
        .add_account_like(&session_address, &target_address)
        .await
        .map_err(|err| {
            info!("Add like Error {:?}", err);
            AppError::BadRequest(err.to_string())
        })?;
    
    Ok(Json(AccountResponse {
        account: updated_account,
    }))
}


#[derive(Debug, Deserialize, ToSchema)]
pub struct RemoveLikeRequest{
    #[schema(example = "target_address")]
    pub target_address: String,
}

/// Remove account like
#[utoipa::path(
    patch,
    path = Path::RemoveAccountLike.as_str(),
    request_body = RemoveLikeRequest,
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Account like successfully", body = AccountResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag="Account"
)]

#[instrument(skip(state, session_address, payload))]
pub async fn remove_account_like(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<RemoveLikeRequest>
)->AppJsonResult<AccountResponse> {
    let target_address = payload.target_address;

    let account_like_controller = AccountLikeController::new(state.postgres.clone());

    let updated_account = account_like_controller
        .remove_account_like(&session_address, &target_address)
        .await
        .map_err(|err| {
            info!("remove like Error {:?}", err);
            AppError::BadRequest(err.to_string())
        })?;

    Ok(Json(AccountResponse{
        account: updated_account,
    }))
}

/// Get account session check
#[utoipa::path(
    get,
    path = Path::GetAccount.as_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication")
    ),
    responses(
        (status = 200, description = "Get account successfully", body = AccountResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag="Account"
)]
#[instrument(skip(state, session_address))]
pub async fn get_account(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<AccountResponse> {
    let account_controller = AccountController::new(state.postgres.clone());
    let account = account_controller
        .get_account(&session_address)
        .await
        .map_err(|err| {
            info!("get account Error {:?}", err);
            AppError::BadRequest(err.to_string())
        })?;

    Ok(Json(AccountResponse {
        account,
    }))
}