use axum::{
    Extension, Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use bytes::Bytes;
use serde::Deserialize;
use tower_cookies::Cookies;
use tracing::{error, info, instrument};

use super::path::DevPostPath;
use crate::{
    middleware::optional_session_address,
    result::{AppError, AppJsonResult, AppResult},
    services::{dev_post::DevPostService, moderation::check_nsfw},
    state::AppState,
    types::{
        common::pagination::{
            DEFAULT_LIMIT, PaginationParams, default_page, deserialize_limit, deserialize_page,
        },
        dev_post::{
            CreateDevPostRequest, DevPostListResponse, DevPostResponse, EditDevPostRequest,
            LikeResponse, RankingResponse, TrendingResponse, UploadImageResponse, VoteRequest,
            VoteResponse,
        },
    },
    utils::{image::sniff_image_format, valid_existing_token_id},
};

const ALLOWED_IMAGE_TYPES: [&str; 4] = ["image/jpeg", "image/png", "image/webp", "image/svg+xml"];
const MAX_IMAGE_SIZE: usize = 5 * 1024 * 1024; // 5MB

// axum's `Query` extractor uses serde_urlencoded, which does not support
// `#[serde(flatten)]` (it buffers values as strings, breaking the int
// deserializers below) — so fields are listed explicitly instead of
// flattening `PaginationParams`. `direction` is omitted: get_feed doesn't use it.
fn default_limit() -> i64 {
    DEFAULT_LIMIT
}

#[derive(Debug, Deserialize)]
pub struct FeedQuery {
    pub token_id: Option<String>,
    #[serde(default = "default_page", deserialize_with = "deserialize_page")]
    pub page: i64,
    #[serde(default = "default_limit", deserialize_with = "deserialize_limit")]
    pub limit: i64,
}

/// GET /dev-post?token_id=&page=&limit=  (public, optional-auth)
#[utoipa::path(
    get,
    path = DevPostPath::Feed.docs_str(),
    params(
        ("token_id" = Option<String>, Query, description = "Filter by token id"),
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Dev post feed fetched successfully", body = DevPostListResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
#[instrument(skip(state, cookies))]
pub async fn get_feed(
    State(state): State<AppState>,
    cookies: Cookies,
    Query(params): Query<FeedQuery>,
) -> AppJsonResult<DevPostListResponse> {
    let token_id = match &params.token_id {
        Some(t) => Some(valid_existing_token_id(&state, t).await?),
        None => None,
    };
    let viewer = optional_session_address(&state, &cookies).await;

    let service = DevPostService::new(state.postgres.clone(), state.redis.clone());
    let feed = service
        .get_feed(
            token_id.as_deref(),
            params.page,
            params.limit,
            viewer.as_deref(),
        )
        .await?;

    Ok(Json(DevPostListResponse {
        pin: feed.pin,
        posts: feed.posts,
        total_count: feed.total_count,
    }))
}

/// GET /dev-post/{post_id}  (public, optional-auth)
#[utoipa::path(
    get,
    path = DevPostPath::Detail.docs_str(),
    params(
        ("post_id" = i64, Path, description = "Dev post ID")
    ),
    responses(
        (status = 200, description = "Dev post fetched successfully", body = DevPostResponse),
        (status = 404, description = "Post not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
#[instrument(skip(state, cookies))]
pub async fn get_detail(
    State(state): State<AppState>,
    cookies: Cookies,
    Path(post_id): Path<i64>,
) -> AppJsonResult<DevPostResponse> {
    let viewer = optional_session_address(&state, &cookies).await;
    let service = DevPostService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_post(post_id, viewer.as_deref()).await?;

    Ok(Json(response))
}

/// GET /dev-post/trending  (public, optional-auth)
#[utoipa::path(
    get,
    path = DevPostPath::Trending.docs_str(),
    responses(
        (status = 200, description = "Trending dev posts fetched successfully", body = TrendingResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
#[instrument(skip(state, cookies))]
pub async fn get_trending(
    State(state): State<AppState>,
    cookies: Cookies,
) -> AppJsonResult<TrendingResponse> {
    let viewer = optional_session_address(&state, &cookies).await;
    let service = DevPostService::new(state.postgres.clone(), state.redis.clone());
    let posts = service.get_trending(viewer.as_deref()).await?;

    Ok(Json(TrendingResponse { posts }))
}

/// GET /dev-post/ranking?page=&limit=  (public, no personalization)
#[utoipa::path(
    get,
    path = DevPostPath::Ranking.docs_str(),
    params(
        ("page" = i64, Query, description = "Page number"),
        ("limit" = i64, Query, description = "Number of items per page")
    ),
    responses(
        (status = 200, description = "Dev post ranking fetched successfully", body = RankingResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
#[instrument(skip(state))]
pub async fn get_ranking(
    State(state): State<AppState>,
    Query(p): Query<PaginationParams>,
) -> AppJsonResult<RankingResponse> {
    let service = DevPostService::new(state.postgres.clone(), state.redis.clone());
    let (rankings, total_count) = service.get_ranking(p.page, p.limit).await?;

    Ok(Json(RankingResponse {
        rankings,
        total_count,
    }))
}

/// POST /dev-post/image  (protected)
#[utoipa::path(
    post,
    path = DevPostPath::UploadImage.docs_str(),
    request_body(
        content = Vec<u8>,
        description = "Raw image binary data. The format is detected from the file's magic bytes \
                       (image/jpeg, image/png, image/webp, image/svg+xml); the Content-Type header is ignored.",
        content_type = "image/png"
    ),
    responses(
        (status = 200, description = "Image uploaded successfully", body = UploadImageResponse),
        (status = 400, description = "Bad request - Bytes are not a supported image format, or image was flagged as NSFW"),
        (status = 413, description = "Payload too large - Image exceeds 5MB limit"),
        (status = 500, description = "Internal server error - Upload failed")
    ),
    tag = "DevPost"
)]
#[instrument(skip(state, body))]
pub async fn upload_image(
    State(state): State<AppState>,
    body: Bytes,
) -> AppJsonResult<UploadImageResponse> {
    info!("Starting devpost image upload - Size: {} bytes", body.len());

    if body.len() > MAX_IMAGE_SIZE {
        error!("Image too large: {} bytes", body.len());
        return Err(AppError::BadRequest(format!(
            "Image size exceeds maximum allowed size of {} MB",
            MAX_IMAGE_SIZE / 1024 / 1024
        )));
    }

    // The request's Content-Type is ignored on purpose — it is client-controlled and
    // becomes the Content-Type R2 serves the object back with.
    let content_type = sniff_image_format(&body, &ALLOWED_IMAGE_TYPES)?;

    // dev-post has no flagging path like /metadata/image — NSFW content is
    // rejected outright, never uploaded.
    if check_nsfw(&body, content_type).await? {
        error!("Devpost image rejected: detected as NSFW");
        return Err(AppError::BadRequest(
            "Image rejected: detected as inappropriate (NSFW) content".to_string(),
        ));
    }

    let image_id = uuid::Uuid::new_v4().to_string();

    let image_uri = state
        .r2
        .upload_devpost_image_file(&image_id, &body, content_type)
        .await
        .map_err(|e| {
            error!("Failed to upload devpost image to R2: {}", e);
            AppError::InternalError(format!("Failed to upload image: {}", e))
        })?;

    info!("Devpost image uploaded successfully: {}", image_uri);

    Ok(Json(UploadImageResponse { image_uri }))
}

/// POST /dev-post  (protected)
#[utoipa::path(
    post,
    path = DevPostPath::Create.docs_str(),
    request_body = CreateDevPostRequest,
    responses(
        (status = 200, description = "Dev post created successfully", body = DevPostResponse),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn create_post(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(mut payload): Json<CreateDevPostRequest>,
) -> AppJsonResult<DevPostResponse> {
    payload.validate().map_err(AppError::BadRequest)?;
    payload.token_id = valid_existing_token_id(&state, &payload.token_id).await?;

    let service = DevPostService::new(state.postgres.clone(), state.redis.clone());
    let id = service.create_post(&session_address, &payload).await?;

    Ok(Json(service.get_post_rw(id, Some(&session_address)).await?))
}

/// PATCH /dev-post/{post_id}  (protected)
#[utoipa::path(
    patch,
    path = DevPostPath::Detail.docs_str(),
    params(
        ("post_id" = i64, Path, description = "Dev post ID")
    ),
    request_body = EditDevPostRequest,
    responses(
        (status = 200, description = "Dev post updated successfully", body = DevPostResponse),
        (status = 400, description = "Bad request"),
        (status = 403, description = "Not the post author"),
        (status = 404, description = "Post not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn edit_post(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(post_id): Path<i64>,
    Json(payload): Json<EditDevPostRequest>,
) -> AppJsonResult<DevPostResponse> {
    payload.validate().map_err(AppError::BadRequest)?;

    let service = DevPostService::new(state.postgres.clone(), state.redis.clone());
    service
        .edit_post(post_id, &session_address, &payload)
        .await?;

    Ok(Json(
        service.get_post_rw(post_id, Some(&session_address)).await?,
    ))
}

/// DELETE /dev-post/{post_id}  (protected)
#[utoipa::path(
    delete,
    path = DevPostPath::Detail.docs_str(),
    params(
        ("post_id" = i64, Path, description = "Dev post ID")
    ),
    responses(
        (status = 200, description = "Dev post deleted successfully"),
        (status = 403, description = "Not the post author"),
        (status = 404, description = "Post not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
#[instrument(skip(state, session_address))]
pub async fn delete_post(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(post_id): Path<i64>,
) -> AppJsonResult<serde_json::Value> {
    let service = DevPostService::new(state.postgres.clone(), state.redis.clone());
    service.delete_post(post_id, &session_address).await?;

    Ok(Json(serde_json::json!({ "deleted": true })))
}

/// POST /dev-post/{post_id}/like  (protected)
#[utoipa::path(
    post,
    path = DevPostPath::Like.docs_str(),
    params(
        ("post_id" = i64, Path, description = "Dev post ID")
    ),
    responses(
        (status = 200, description = "Post liked successfully", body = LikeResponse),
        (status = 404, description = "Post not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
#[instrument(skip(state, session_address))]
pub async fn like(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(post_id): Path<i64>,
) -> AppJsonResult<LikeResponse> {
    let service = DevPostService::new(state.postgres.clone(), state.redis.clone());
    let like_count = service.like(post_id, &session_address).await?;

    Ok(Json(LikeResponse {
        like_count,
        liked_by_me: true,
    }))
}

/// DELETE /dev-post/{post_id}/like  (protected)
#[utoipa::path(
    delete,
    path = DevPostPath::Like.docs_str(),
    params(
        ("post_id" = i64, Path, description = "Dev post ID")
    ),
    responses(
        (status = 200, description = "Post unliked successfully", body = LikeResponse),
        (status = 404, description = "Post not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
#[instrument(skip(state, session_address))]
pub async fn unlike(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(post_id): Path<i64>,
) -> AppJsonResult<LikeResponse> {
    let service = DevPostService::new(state.postgres.clone(), state.redis.clone());
    let like_count = service.unlike(post_id, &session_address).await?;

    Ok(Json(LikeResponse {
        like_count,
        liked_by_me: false,
    }))
}

/// POST /dev-post/{post_id}/vote  (protected)
#[utoipa::path(
    post,
    path = DevPostPath::Vote.docs_str(),
    params(
        ("post_id" = i64, Path, description = "Dev post ID")
    ),
    request_body = VoteRequest,
    responses(
        (status = 200, description = "Vote submitted successfully", body = VoteResponse),
        (status = 400, description = "Bad request"),
        (status = 404, description = "Post or poll not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
#[instrument(skip(state, session_address, payload))]
pub async fn vote(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(post_id): Path<i64>,
    Json(payload): Json<VoteRequest>,
) -> AppJsonResult<VoteResponse> {
    let service = DevPostService::new(state.postgres.clone(), state.redis.clone());
    service
        .vote(post_id, &session_address, payload.option_position)
        .await?;

    let post = service.get_post_rw(post_id, Some(&session_address)).await?;
    let poll = post
        .poll
        .ok_or_else(|| AppError::NotFound("Poll not found".into()))?;

    Ok(Json(VoteResponse {
        total_votes: poll.total_votes,
        options: poll.options,
        my_vote_option: poll.my_vote_option,
    }))
}

#[utoipa::path(
    put,
    path = DevPostPath::Pin.docs_str(),
    params(("post_id" = i64, Path, description = "Dev post ID")),
    responses(
        (status = 204, description = "Post pinned or existing pin replaced"),
        (status = 400, description = "Malformed post ID"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Current creator or author requirement not met"),
        (status = 404, description = "Live post or token not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
pub async fn pin_post(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(post_id): Path<i64>,
) -> AppResult<StatusCode> {
    DevPostService::new(state.postgres.clone(), state.redis.clone())
        .pin_post(post_id, &session_address)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = DevPostPath::Pin.docs_str(),
    params(("post_id" = i64, Path, description = "Dev post ID")),
    responses(
        (status = 204, description = "Exact pin removed or already absent"),
        (status = 400, description = "Malformed post ID"),
        (status = 401, description = "Authentication required"),
        (status = 403, description = "Current creator requirement not met"),
        (status = 404, description = "Live post or token not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "DevPost"
)]
pub async fn unpin_post(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Path(post_id): Path<i64>,
) -> AppResult<StatusCode> {
    DevPostService::new(state.postgres.clone(), state.redis.clone())
        .unpin_post(post_id, &session_address)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::extract::Query;

    // Exercises the real axum Query extractor (serde_urlencoded under the hood),
    // not a hand-built struct — this is the seam controller/service tests bypass.
    #[test]
    fn feed_query_parses_page_and_limit_from_url() {
        let uri: axum::http::Uri = "http://x/dev-post?token_id=0xAbc&page=2&limit=5"
            .parse()
            .unwrap();
        let q: Query<FeedQuery> = Query::try_from_uri(&uri).unwrap();
        assert_eq!(q.page, 2);
        assert_eq!(q.limit, 5);
        assert_eq!(q.token_id.as_deref(), Some("0xAbc"));
    }

    #[test]
    fn feed_query_defaults_when_omitted() {
        let uri: axum::http::Uri = "http://x/dev-post".parse().unwrap();
        let q: Query<FeedQuery> = Query::try_from_uri(&uri).unwrap();
        assert_eq!(q.page, 1);
        assert_eq!(q.limit, 10);
        assert_eq!(q.token_id, None);
    }
}
