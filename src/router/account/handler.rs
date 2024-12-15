use axum::{
    extract::{Multipart, State},
    Extension, Json, 
};
use bytes::Bytes;
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
    pub image: Option<Bytes>,
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

    // 필드 파싱을 위한 헬퍼 함수
    async fn parse_field(field: axum::extract::multipart::Field<'_>) -> Result<(String, Option<String>, Option<(Bytes, String)>), AppError> {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "data" => {
                let data = field.text().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                Ok((name, Some(data), None))
            },
            "image" => {
                let content_type = field.content_type()
                    .map(|ct| ct.to_string())
                    .unwrap_or_else(|| {
                        if field.file_name().map(|f| f.ends_with(".png")).unwrap_or(false) {
                            "image/png".to_string()
                        } else {
                            "image/jpeg".to_string()
                        }
                    });
                let bytes = field.bytes().await.map_err(|e| AppError::BadRequest(e.to_string()))?;
                Ok((name, None, Some((bytes, content_type))))
            },
            _ => Ok((name, None, None))
        }
    }

    // multipart 데이터 파싱
    let mut form_data = None;
    let mut image_info = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| AppError::BadRequest(e.to_string()))? {
        let (name, text_data, file_data) = parse_field(field).await?;
        match name.as_str() {
            "data" if text_data.is_some() => {
                form_data = Some(serde_json::from_str::<UpdateAccountRequest>(
                    &text_data.unwrap()
                ).map_err(|e| AppError::BadRequest(e.to_string()))?);
            },
            "image" if file_data.is_some() => {
                image_info = file_data;
            },
            _ => {}
        }
    }

    // 요청 데이터 검증
    let form_data = form_data.ok_or_else(|| AppError::BadRequest("Missing account data".into()))?;
    if form_data.nick_name.is_none() && form_data.bio.is_none() && image_info.is_none() {
        return Err(AppError::BadRequest("At least one of nickName, bio, or image must be provided".into()));
    }

    // 텍스트 필드 처리
    let clean_text = |text: Option<String>| {
        text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
    };

    // 이미지 업로드
    let image_uri = if let Some((image_data, content_type)) = image_info {
        info!("Uploading image with content-type: {}", content_type);
        Some(state.s3_client
            .upload_profile_image_file(&session_address, image_data, content_type)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?)
    } else {
        None
    };

    // 계정 업데이트
    let account_controller = AccountController::new(state.postgres.clone());
    let updated_account = account_controller
        .update_account(
            &session_address,
            image_uri,
            clean_text(form_data.nick_name),
            clean_text(form_data.bio)
        )
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    Ok(Json(AccountResponse { account: updated_account }))
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
    info!("Get account for account: {}", session_address);
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