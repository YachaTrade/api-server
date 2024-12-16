use axum::{
    extract::{Multipart, Path, State},
    Extension, Json,
};
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use tracing::{info, instrument};
use utoipa::ToSchema;

use crate::{
    db::postgres::{controller::thread::ThreadController, model::Thread},
    result::{AppError, AppJsonResult},
    state::AppState,
};

use super::path::Path as ThreadPath;

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateThreadRequest {
    token_id: String,
    content: String,
    parent_id: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ThreadResponse {
    thread: Thread,
}
#[derive(ToSchema)]
pub struct CreateThreadFormData {
    #[schema(example = json!({
        "token_id": "token_address",
        "content": "Your Content",
        "root_id": "Null or root Thread ID"
    }))]
    pub data: String, // JSON string

    #[schema(format = "binary")]
    pub image: Option<Bytes>,
}

/// Create thread
#[utoipa::path(
    post,
    path = ThreadPath::CreateThread.docs_str(),
    params(
       ("session" = String, Cookie, description = "Session token for authentication")
    ),
    request_body(
        content = CreateThreadFormData,
        content_type = "multipart/form-data",
        description = "Create Thread data and image",
        example = json!({
            "data": {
                "token_id": "coin address",
                "content":"Your Content",
                "root_id": "Null or Parent Thread ID"
            },
            "image": "[binary]"
        })
    ),
    responses(
        (status = 200, description = "Thread created successfully", body = ThreadResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag = "Thread"
)]
#[instrument(skip(state, session_address, multipart))]
pub async fn create_thread(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    mut multipart: Multipart,
) -> AppJsonResult<ThreadResponse> {
    info!("Creating thread for user: {}", session_address);

    // 필드 파싱을 위한 헬퍼 함수
    async fn parse_field(
        field: axum::extract::multipart::Field<'_>,
    ) -> Result<(String, Option<String>, Option<(Bytes, String)>), AppError> {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "data" => {
                let data = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                Ok((name, Some(data), None))
            }
            "image" => {
                let content_type = field
                    .content_type()
                    .map(|ct| ct.to_string())
                    .unwrap_or_else(|| "image/jpeg".to_string());
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(e.to_string()))?;
                Ok((name, None, Some((bytes, content_type))))
            }
            _ => Ok((name, None, None)),
        }
    }

    // multipart 데이터 파싱
    let mut form_data = None;
    let mut image_info = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?
    {
        let (name, text_data, file_data) = parse_field(field).await?;
        match name.as_str() {
            "data" if text_data.is_some() => {
                form_data = Some(
                    serde_json::from_str::<CreateThreadRequest>(&text_data.unwrap())
                        .map_err(|e| AppError::BadRequest(e.to_string()))?,
                );
            }
            "image" if file_data.is_some() => {
                image_info = file_data;
            }
            _ => {}
        }
    }

    // 요청 데이터 검증
    let form_data = form_data.ok_or_else(|| AppError::BadRequest("Missing thread data".into()))?;
    let thread_controller = ThreadController::new(state.postgres.clone());

    // 이미지 업로드
    let image_uri = if let Some((image_data, content_type)) = image_info {
        let last_thread_id = thread_controller.get_last_thread_id().await?;
        let next_thread_id = if last_thread_id == 0 {
            0
        } else {
            last_thread_id + 1
        };

        info!("Uploading thread image with content-type: {}", content_type);
        Some(
            state
                .s3_client
                .upload_thread_image_file(
                    &session_address,
                    next_thread_id,
                    image_data,
                    &content_type,
                )
                .await?,
        )
    } else {
        None
    };
    // 스레드 생성
    let thread = thread_controller
        .create_thread(
            form_data.token_id,
            session_address,
            form_data.content,
            form_data.parent_id,
            image_uri,
        )
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    Ok(Json(ThreadResponse { thread }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ThreadRequest {
    #[schema(example = 1)]
    thread_id: i32,
    token_id: String,
}

/// Like thread
#[utoipa::path(
    post,
    path = ThreadPath::LikeThread.docs_str(),
    params(
       ("session" = String, Cookie, description = "Session token for authentication")
    ),
    request_body = ThreadRequest,
    responses(
        (status = 200, description = "Thread liked successfully", body = ThreadResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag="Thread"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn like_thread(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<ThreadRequest>,
) -> AppJsonResult<ThreadResponse> {
    let ThreadRequest {
        thread_id,
        token_id,
    } = payload;

    let thread_controller = ThreadController::new(state.postgres.clone());
    let thread = thread_controller
        .like_thread(thread_id, &session_address, &token_id)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    Ok(Json(ThreadResponse { thread }))
}
/// Unlike thread
#[utoipa::path(
    delete,
    path = ThreadPath::UnLikeThread.docs_str(),
    params(
       ("session" = String, Cookie, description = "Session token for authentication")
    ),
    request_body = ThreadRequest,
    responses(
        (status = 200, description = "Thread unliked successfully", body = ThreadResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session_token" = [])
    ),
    tag="Thread"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn unlike_thread(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<ThreadRequest>,
) -> AppJsonResult<ThreadResponse> {
    let ThreadRequest {
        thread_id,
        token_id,
    } = payload;

    let thread_controller = ThreadController::new(state.postgres.clone());
    let thread = thread_controller
        .unlike_thread(thread_id, &session_address, &token_id)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    Ok(Json(ThreadResponse { thread }))
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ThreadLikeResponse {
    pub thread_ids: Vec<i32>,
}

/// Get thread likes by account and token
#[utoipa::path(
    get,
    path = ThreadPath::GetThreadLike.docs_str(),
    params(
        ("token_id" = String, Path, description = "Token ID to get likes for"),
        ("session" = String, Cookie, description = "Session token for authentication")
    ),
    responses(
        (status = 200, description = "Successfully retrieved thread likes", body = ThreadLikeResponse),
        (status = 400, description = "Bad request"),
        (status = 401, description = "Unauthorized"),
        (status = 500, description = "Internal server error")
    ),
    security(
        ("session" = [])
    ),
    tag="Thread"
)]
#[instrument(skip(state, session_address))]
pub async fn get_thread_like_by_account(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(token_id): Path<String>, // 경로 파라미터로 token_id 추가
) -> AppJsonResult<ThreadLikeResponse> {
    // token_id check
    if !token_id.starts_with("0x")
        || token_id.len() != 42
        || !token_id[2..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(AppError::BadRequest(
            "Invalid token address format".to_string(),
        ));
    }
    let thread_controller = ThreadController::new(state.postgres.clone());

    let thread_ids = thread_controller
        .get_thread_like_by_token_and_account(&session_address, &token_id)
        .await
        .map_err(|err| AppError::InternalError(err.to_string()))?;

    Ok(Json(ThreadLikeResponse { thread_ids }))
}
