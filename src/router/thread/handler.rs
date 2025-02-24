use axum::{
    extract::{Multipart, Path, Query, State},
    Extension, Json,
};
use bytes::Bytes;
use serde::Serialize;
use tracing::{error, info, instrument};
use utoipa::ToSchema;

use crate::{
    result::{AppError, AppJsonResult},
    state::AppState,
    types::{
        common::pagination::PaginationParams,
        social::thread::{
            CreateThreadRequest, ThreadController, ThreadLikeResponse, ThreadRequest,
            ThreadResponse, ThreadsResponse,
        },
    },
    utils::valid_evm_address,
};

use super::path::Path as ThreadPath;

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
#[instrument(skip(state))]
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

    if form_data.content.contains("http") || form_data.content.contains("t.me") {
        return Err(AppError::BadRequest(
            "Content cannot contain 'http' or 't.me'".into(),
        ));
    }

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
            session_address.clone(),
            form_data.content,
            form_data.parent_id,
            image_uri,
        )
        .await
        .map_err(|err| {
            error!(
                "Failed to create thread: session_address: {}, error: {}",
                session_address, err
            );
            AppError::InternalError(err.to_string())
        })?;
    info!(
        "Create Thread: session_address: {}, response: {:?}",
        session_address, thread
    );
    Ok(Json(ThreadResponse { thread }))
}

/// Get threads by token ID
#[utoipa::path(
    get,
    path = ThreadPath::GetThread.docs_str(),
    params(
        ("token_id" = String, Path, description = "EVM compatible token address"),
        ("page" = Option<i64>, Query, description = "Page number for pagination"),
        ("limit" = Option<i64>, Query, description = "Number of items limit")
    ),
    responses(
        (status = 200, description = "Successfully retrieved threads", body = ThreadsResponse),
        (status = 400, description = "Invalid token ID format"),
        (status = 500, description = "Internal server error")
    ),
    tag = "Thread"
)]
#[instrument(skip(state))]
pub async fn get_threads_by_token(
    Path(token_id): Path<String>,
    Query(params): Query<PaginationParams>,
    State(state): State<AppState>,
) -> AppJsonResult<ThreadsResponse> {
    if !valid_evm_address(&token_id) {
        return Err(AppError::BadRequest("Invalid token ID".to_string()));
    }
    let thread_controller = ThreadController::new(state.postgres.clone());
    let response = thread_controller
        .get_threads_by_token(&token_id, params)
        .await
        .map_err(|err| {
            error!(
                "Failed to get threads by token: token_id: {}, error: {}",
                token_id, err
            );
            AppError::InternalError(err.to_string())
        })?;
    info!(
        "Get Threads By Token: token_id: {}, response: {:?}",
        token_id, response
    );
    Ok(Json(response))
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
#[instrument(skip(state))]
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
        .map_err(|err| {
            error!(
                "Failed to like thread: thread_id: {}, session_address: {}, error: {}",
                thread_id, session_address, err
            );
            AppError::BadRequest(err.to_string())
        })?;
    info!(
        "Like Thread: thread_id: {}, session_address: {}, response: {:?}",
        thread_id, session_address, thread
    );
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
#[instrument(skip(state))]
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
        .map_err(|err| {
            error!(
                "Failed to unlike thread: thread_id: {}, session_address: {}, error: {}",
                thread_id, session_address, err
            );
            AppError::BadRequest(err.to_string())
        })?;
    info!(
        "Unlike Thread: thread_id: {}, session_address: {}, response: {:?}",
        thread_id, session_address, thread
    );
    Ok(Json(ThreadResponse { thread }))
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
#[instrument(skip(state))]
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
        .map_err(|err| {
            error!("Failed to get thread likes by account: session_address: {}, token_id: {}, error: {}", session_address, token_id, err);
            AppError::InternalError(err.to_string())
        })?;
    info!(
        "Get Thread Likes By Account: session_address: {}, token_id: {}, response: {:?}",
        session_address, token_id, thread_ids
    );
    Ok(Json(ThreadLikeResponse { thread_ids }))
}
