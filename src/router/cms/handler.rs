use axum::{
    Extension, Json,
    extract::{Multipart, State},
};
use bytes::Bytes;
use tracing::{error, info, instrument};

use super::path::CmsPath;
use crate::{
    result::{AppError, AppJsonResult},
    services::cms::CmsService,
    state::AppState,
    types::cms::{
        CmsActionResponse, DexTokenImageResponse, InsertTrendRequest, SetNsfwRequest,
        UpdateMetadataRequest, UpdateMetadataResponse, WhitelistTokenListResponse,
    },
};

/// Set token NSFW status (Admin only)
#[utoipa::path(
    post,
    path = CmsPath::SetNsfw.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication (Admin only)")
    ),
    request_body = SetNsfwRequest,
    responses(
        (status = 200, description = "NSFW status updated successfully", body = CmsActionResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS"
)]
#[instrument(skip(state, payload))]
pub async fn set_nsfw(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<SetNsfwRequest>,
) -> AppJsonResult<CmsActionResponse> {
    payload.validate().map_err(AppError::BadRequest)?;

    let service = CmsService::new(state.postgres.clone(), state.r2.clone());
    let response = service.set_nsfw(&session_address, payload).await?;

    Ok(Json(response))
}

/// Insert trend tokens (Admin only)
#[utoipa::path(
    post,
    path = CmsPath::InsertTrend.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication (Admin only)")
    ),
    request_body = InsertTrendRequest,
    responses(
        (status = 200, description = "Trend tokens inserted successfully", body = CmsActionResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS"
)]
#[instrument(skip(state, payload))]
pub async fn insert_trend(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    Json(payload): Json<InsertTrendRequest>,
) -> AppJsonResult<CmsActionResponse> {
    payload.validate().map_err(AppError::BadRequest)?;

    let service = CmsService::new(state.postgres.clone(), state.r2.clone());
    let response = service.insert_trend(&session_address, payload).await?;

    Ok(Json(response))
}

/// Update token metadata (Admin only)
/// Accepts multipart/form-data with optional fields: token_id (required), description, website, twitter, telegram, image
#[utoipa::path(
    post,
    path = CmsPath::UpdateMetadata.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication (Admin only)")
    ),
    request_body(
        content_type = "multipart/form-data",
        content = UpdateMetadataMultipart,
    ),
    responses(
        (status = 200, description = "Metadata updated successfully", body = UpdateMetadataResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 400, description = "Bad request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS"
)]
#[instrument(skip(state, multipart))]
pub async fn update_metadata(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    mut multipart: Multipart,
) -> AppJsonResult<UpdateMetadataResponse> {
    let mut token_id: Option<String> = None;
    let mut description: Option<String> = None;
    let mut website: Option<String> = None;
    let mut twitter: Option<String> = None;
    let mut telegram: Option<String> = None;
    let mut image_data: Option<Bytes> = None;

    // Parse multipart form data
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!("Failed to read multipart field: {}", e);
        AppError::BadRequest(format!("Failed to read form data: {}", e))
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "token_id" => {
                token_id = Some(field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read token_id: {}", e))
                })?);
            }
            "description" => {
                description = Some(field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read description: {}", e))
                })?);
            }
            "website" => {
                website =
                    Some(field.text().await.map_err(|e| {
                        AppError::BadRequest(format!("Failed to read website: {}", e))
                    })?);
            }
            "twitter" => {
                twitter =
                    Some(field.text().await.map_err(|e| {
                        AppError::BadRequest(format!("Failed to read twitter: {}", e))
                    })?);
            }
            "telegram" => {
                telegram = Some(field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read telegram: {}", e))
                })?);
            }
            "image" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read image: {}", e)))?;
                // Only set image_data if not empty
                if !bytes.is_empty() {
                    image_data = Some(bytes);
                }
            }
            _ => {
                info!("Ignoring unknown field: {}", name);
            }
        }
    }

    // Validate required field
    let token_id =
        token_id.ok_or_else(|| AppError::BadRequest("token_id is required".to_string()))?;

    let request = UpdateMetadataRequest {
        token_id,
        description,
        website,
        twitter,
        telegram,
    };

    request.validate().map_err(AppError::BadRequest)?;

    let service = CmsService::new(state.postgres.clone(), state.r2.clone());
    let response = service
        .update_metadata(&session_address, request, image_data)
        .await?;

    Ok(Json(response))
}

/// Upload a dex_token logo image (Admin only)
/// Accepts multipart/form-data: token_id (required), image (required).
///
/// Scope: updates `dex_token.image_uri` only. DEX reads resolve images via
/// `COALESCE(token.image_uri, dex_token.image_uri, quote_token.image_uri)`, so for a
/// token that also has a `token` row (launchpad/graduated) the dex_token image is
/// shadowed — manage those via `/cms/token/metadata` instead. This endpoint targets
/// pure DEX tokens (external, discovered via PairCreated, no `token` row).
#[utoipa::path(
    post,
    path = CmsPath::UploadDexTokenImage.docs_str(),
    params(
        ("session" = String, Cookie, description = "Session cookie for authentication (Admin only)")
    ),
    request_body(
        content_type = "multipart/form-data",
        content = UploadDexTokenImageMultipart,
    ),
    responses(
        (status = 200, description = "Image uploaded and dex_token updated", body = DexTokenImageResponse),
        (status = 401, description = "Unauthorized - Not an admin"),
        (status = 400, description = "Bad request - invalid token_id or image"),
        (status = 404, description = "dex_token not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS"
)]
#[instrument(skip(state, multipart))]
pub async fn upload_dex_token_image(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    mut multipart: Multipart,
) -> AppJsonResult<DexTokenImageResponse> {
    let mut token_id: Option<String> = None;
    let mut image_data: Option<Bytes> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!("Failed to read multipart field: {}", e);
        AppError::BadRequest(format!("Failed to read form data: {}", e))
    })? {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "token_id" => {
                token_id = Some(field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read token_id: {}", e))
                })?);
            }
            "image" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read image: {}", e)))?;
                if !bytes.is_empty() {
                    image_data = Some(bytes);
                }
            }
            _ => {
                info!("Ignoring unknown field: {}", name);
            }
        }
    }

    let token_id =
        token_id.ok_or_else(|| AppError::BadRequest("token_id is required".to_string()))?;
    let image_data =
        image_data.ok_or_else(|| AppError::BadRequest("image is required".to_string()))?;

    let service = CmsService::new(state.postgres.clone(), state.r2.clone());
    let response = service
        .update_dex_token_image(&session_address, &token_id, image_data)
        .await?;

    Ok(Json(response))
}

/// POST /cms/whitelist-token — 화이트리스트 토큰 upsert (admin, multipart).
/// Accepts multipart/form-data: token_id (required), sort_order (required),
/// enabled (optional bool, default true), name, symbol, price_feed_id, decimals (optional),
/// image (optional file).
#[utoipa::path(
    post,
    path = "/cms/whitelist-token",
    params(
        ("session" = String, Cookie, description = "Session cookie (Admin only)")
    ),
    request_body(
        content_type = "multipart/form-data",
        content = UpsertWhitelistTokenMultipart,
    ),
    responses(
        (status = 200, description = "Upserted", body = CmsActionResponse),
        (status = 400, description = "Invalid token_id or missing required field"),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS"
)]
#[instrument(skip(state, multipart))]
pub async fn upsert_whitelist_token(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
    mut multipart: Multipart,
) -> AppJsonResult<CmsActionResponse> {
    let mut token_id: Option<String> = None;
    let mut sort_order: Option<i32> = None;
    let mut enabled: Option<bool> = None;
    let mut name: Option<String> = None;
    let mut symbol: Option<String> = None;
    let mut price_feed_id: Option<String> = None;
    let mut decimals: Option<i32> = None;
    let mut image_data: Option<Bytes> = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!("Failed to read multipart field: {}", e);
        AppError::BadRequest(format!("Failed to read form data: {}", e))
    })? {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "token_id" => {
                token_id = Some(field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read token_id: {}", e))
                })?);
            }
            "sort_order" => {
                let s = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read sort_order: {}", e))
                })?;
                sort_order = Some(s.trim().parse::<i32>().map_err(|_| {
                    AppError::BadRequest("sort_order must be an integer".to_string())
                })?);
            }
            "enabled" => {
                let s = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read enabled: {}", e))
                })?;
                enabled = Some(s.trim().parse::<bool>().map_err(|_| {
                    AppError::BadRequest("enabled must be true or false".to_string())
                })?);
            }
            "name" => {
                let v = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read name: {}", e))
                })?;
                if !v.is_empty() { name = Some(v); }
            }
            "symbol" => {
                let v = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read symbol: {}", e))
                })?;
                if !v.is_empty() { symbol = Some(v); }
            }
            "price_feed_id" => {
                let v = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read price_feed_id: {}", e))
                })?;
                if !v.is_empty() { price_feed_id = Some(v); }
            }
            "decimals" => {
                let s = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read decimals: {}", e))
                })?;
                if !s.trim().is_empty() {
                    decimals = Some(s.trim().parse::<i32>().map_err(|_| {
                        AppError::BadRequest("decimals must be an integer".to_string())
                    })?);
                }
            }
            "image" => {
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read image: {}", e)))?;
                if !bytes.is_empty() {
                    image_data = Some(bytes);
                }
            }
            _ => {
                info!("Ignoring unknown field: {}", field_name);
            }
        }
    }

    let token_id =
        token_id.ok_or_else(|| AppError::BadRequest("token_id is required".to_string()))?;
    let sort_order =
        sort_order.ok_or_else(|| AppError::BadRequest("sort_order is required".to_string()))?;
    let enabled = enabled.unwrap_or(true);

    let service = CmsService::new(state.postgres.clone(), state.r2.clone());
    let response = service
        .upsert_whitelist_token(
            &session_address,
            &token_id,
            sort_order,
            enabled,
            name,
            symbol,
            price_feed_id,
            decimals,
            image_data,
        )
        .await?;
    Ok(Json(response))
}

/// GET /cms/whitelist-token — 화이트리스트 목록 (admin).
#[utoipa::path(
    get,
    path = "/cms/whitelist-token",
    responses(
        (status = 200, description = "Whitelist tokens", body = WhitelistTokenListResponse),
        (status = 403, description = "Admin access required"),
        (status = 500, description = "Internal server error")
    ),
    tag = "CMS"
)]
#[instrument(skip(state, session_address))]
pub async fn list_whitelist_token(
    State(state): State<AppState>,
    Extension(session_address): Extension<String>,
) -> AppJsonResult<WhitelistTokenListResponse> {
    let service = CmsService::new(state.postgres.clone(), state.r2.clone());
    let response = service.list_whitelist_tokens(&session_address).await?;
    Ok(Json(response))
}

/// Schema for whitelist-token upsert multipart form
#[derive(utoipa::ToSchema)]
pub struct UpsertWhitelistTokenMultipart {
    /// Token address (required)
    pub token_id: String,
    /// Sort order (required)
    pub sort_order: i32,
    /// Enabled (optional, default true)
    #[schema(nullable = true)]
    pub enabled: Option<bool>,
    #[schema(nullable = true)]
    pub name: Option<String>,
    #[schema(nullable = true)]
    pub symbol: Option<String>,
    #[schema(nullable = true)]
    pub price_feed_id: Option<String>,
    #[schema(nullable = true)]
    pub decimals: Option<i32>,
    /// Image file (optional)
    #[schema(value_type = Option<String>, format = Binary, nullable = true)]
    pub image: Option<String>,
}

/// Schema for dex_token image multipart form
#[derive(utoipa::ToSchema)]
pub struct UploadDexTokenImageMultipart {
    /// dex_token address (required)
    pub token_id: String,
    /// Image file (required)
    #[schema(value_type = String, format = Binary)]
    pub image: String,
}

/// Schema for update metadata multipart form
#[derive(utoipa::ToSchema)]
pub struct UpdateMetadataMultipart {
    /// Token address (required)
    pub token_id: String,
    /// Token description (optional)
    #[schema(nullable = true)]
    pub description: Option<String>,
    /// Website URL (optional, must start with https://)
    #[schema(nullable = true)]
    pub website: Option<String>,
    /// Twitter/X URL (optional, must start with https://x.com/)
    #[schema(nullable = true)]
    pub twitter: Option<String>,
    /// Telegram URL (optional, must start with https://t.me/)
    #[schema(nullable = true)]
    pub telegram: Option<String>,
    /// Image file (optional)
    #[schema(value_type = Option<String>, format = Binary, nullable = true)]
    pub image: Option<String>,
}
