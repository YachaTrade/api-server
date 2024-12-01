use axum::{
    extract::{Multipart, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, instrument};
use utoipa::ToSchema;

use crate::{
    db::postgres::{controller::thread::ThreadController, model::Thread},
    result::{AppError, AppJsonResult},
    state::AppState,
};

use super::path::Path;

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
    pub image: Option<Vec<u8>>,
}

/// Create thread
#[utoipa::path(
    post,
    path = Path::CreateThread.as_str(),
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
    let account_id = session_address;
    let mut form_data = None;
    let mut image_data = None;
    let mut content_type = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();
        // content_type = field.content_type().unwrap().to_string();
        // info!("name = {}", name);
        if name == "data" {
            let data: String = field
                .text()
                .await
                .map_err(|err| AppError::BadRequest(err.to_string()))?;
            // info!("data = {}", data);
            form_data = Some(
                serde_json::from_str::<CreateThreadRequest>(&data)
                    .map_err(|err| AppError::BadRequest(err.to_string()))?,
            );
        } else if name == "image" {
            content_type = field.content_type().map(|ct| ct.to_string());
            info!("content_type = {:?}", content_type);
            image_data = Some(
                field
                    .bytes()
                    .await
                    .map_err(|err| AppError::BadRequest(err.to_string()))?,
            );
        }
    }
    let form_data = form_data.ok_or_else(|| AppError::BadRequest("Missing thread data".into()))?;
    let thread_controller = ThreadController::new(state.postgres.clone());

    let image_uri = if let Some(image_data) = image_data {
        let r2_client = state.r2client.clone();
        let content_type = content_type.unwrap_or("image/jpg".to_string());
        let last_thread_id = thread_controller.get_last_thread_id().await?;

        // last_thread_id가 0이면 첫 번째 스레드이므로 0 유지
        // 0이 아니면 last_thread_id + 1
        let next_thread_id = if last_thread_id == 0 {
            0
        } else {
            last_thread_id + 1
        };

        let image_uri = r2_client
            .upload_thread_image_file(&account_id, next_thread_id, image_data, &content_type)
            .await?;
        Some(image_uri)
    } else {
        None
    };

    let thread = thread_controller
        .create_thread(
            form_data.token_id,
            account_id.clone(),
            form_data.content,
            form_data.parent_id,
            image_uri,
        )
        .await?;

    Ok(Json(ThreadResponse { thread }))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ThreadRequest {
    #[schema(example = 1)]
    thread_id: i32,
}

/// Like thread
#[utoipa::path(
    post,
    path = Path::LikeThread.as_str(),
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
    let ThreadRequest { thread_id } = payload;

    let thread_controller = ThreadController::new(state.postgres.clone());
    let thread = thread_controller
        .like_thread(thread_id, &session_address)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    Ok(Json(ThreadResponse { thread }))
}
/// Unlike thread
#[utoipa::path(
    post,
    path = Path::UnLikeThread.as_str(),
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
    let ThreadRequest { thread_id } = payload;

    let thread_controller = ThreadController::new(state.postgres.clone());
    let thread = thread_controller
        .unlike_thread(thread_id, &session_address)
        .await
        .map_err(|err| AppError::BadRequest(err.to_string()))?;

    Ok(Json(ThreadResponse { thread }))
}
