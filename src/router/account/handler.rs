use axum::{
    extract::{Multipart, State},
    Extension, Json, 
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};
use utoipa::ToSchema;

use crate::{
     result::{AppError, AppJsonResult}, state::AppState, types::account::{Account, AccountController, AccountResponse, UpdateAccountRequest}

};

use super::path::AccountPath;



/// Update account profile
#[utoipa::path(
    patch,
    path = AccountPath::UpdateAccount.docs_str(),
    request_body(
        content = UpdateAccountFormData,
        content_type = "multipart/form-data",
        description = "Account update nickname and image",
        example = json!({ 
            "data": {
                "nickname": "Your Nickname"
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
    if form_data.nickname.is_none() && form_data.bio.is_none() && image_info.is_none() {
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
            clean_text(form_data.nickname),
            clean_text(form_data.bio)
        )
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;

    Ok(Json(AccountResponse { account: updated_account }))
}





/// Get account session check
#[utoipa::path(
    get,
    path = AccountPath::GetAccount.docs_str(),
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