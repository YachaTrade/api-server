use std::env;

use crate::result::AppError;
use anyhow::Result;
use axum::extract::Multipart;
use bytes::Bytes;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{error, info};
use utoipa::ToSchema;

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct BotMetadataRequest {
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub image_uri: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BotMetadataResponse {
    pub metadata_url: String,
}

// 허용된 이미지 타입 상수 정의
const ALLOWED_IMAGE_TYPES: [&str; 9] = [
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/bmp",
    "image/tiff",
    "image/heic",
    "image/heif",
    "image/avif",
];
// 파일 크기 제한 상수 정의
const MAX_FILE_SIZE: usize = 5 * 1024 * 1024; // 5MB

// Worker API URL 관련 함수
fn get_worker_api_url() -> Result<String, AppError> {
    env::var("WORKER_API_URL").map_err(|_| {
        AppError::InternalError("WORKER_API_URL environment variable not found".to_string())
    })
}

pub async fn parse_multipart_data(
    multipart: &mut Multipart,
) -> Result<(BotMetadataRequest, (Bytes, String)), AppError> {
    let mut image_file: Option<(Bytes, String)> = None;
    let mut metadata: Option<BotMetadataRequest> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to process multipart form: {}", e)))?
    {
        let name = field.name().unwrap_or_default().to_string();

        match name.as_str() {
            "image" => {
                let content_type = field.content_type().unwrap_or("image/png").to_string();

                // Validate image type
                if !ALLOWED_IMAGE_TYPES.contains(&content_type.as_str()) {
                    return Err(AppError::BadRequest(format!(
                        "Unsupported image type: {}",
                        content_type
                    )));
                }

                let data = field.bytes().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read image file: {}", e))
                })?;

                // Validate file size
                if data.len() > MAX_FILE_SIZE {
                    return Err(AppError::BadRequest(format!(
                        "Image file size exceeds the 5MB limit: {} bytes",
                        data.len()
                    )));
                }

                image_file = Some((data, content_type));
            }
            "metadata" => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Failed to read metadata: {}", e)))?;

                let metadata_str = String::from_utf8(data.to_vec())
                    .map_err(|e| AppError::BadRequest(format!("Invalid metadata format: {}", e)))?;

                metadata =
                    Some(serde_json::from_str(&metadata_str).map_err(|e| {
                        AppError::BadRequest(format!("Invalid metadata JSON: {}", e))
                    })?);
            }
            _ => {
                info!("Ignoring unknown field: {}", name);
            }
        }
    }

    // Validate required fields
    let metadata = metadata.ok_or_else(|| AppError::BadRequest("Missing metadata".to_string()))?;
    let image_file =
        image_file.ok_or_else(|| AppError::BadRequest("Missing image file".to_string()))?;

    Ok((metadata, image_file))
}

pub async fn request_upload_image_url(
    file_name: &str,
    content_type: &str,
    file_size: usize,
) -> Result<Value> {
    let client = Client::new();
    let worker_api_url = env::var("WORKER_API_URL").expect("WORKER_API_URL not found");

    let image_upload_url = format!("{}/get-upload-url", worker_api_url);

    let response = client
        .post(&image_upload_url)
        .json(&json!({
            "fileName": file_name,
            "fileType": content_type,
            "fileSize": file_size
        }))
        .send()
        .await
        .map_err(|e| {
            error!("Failed to get upload URL: {}", e);
            anyhow::anyhow!("Failed to get upload URL: {}", e)
        })?;
    // info!("response: {:#?}", response);
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        error!("Worker API responded with error: {} - {}", status, text);
        return Err(anyhow::anyhow!("Failed to get upload URL: {}", text));
    }

    info!("Upload URL requested successfully");
    response.json().await.map_err(|e| {
        error!("Failed to parse upload URL response: {}", e);
        anyhow::anyhow!("Failed to parse upload URL response: {}", e)
    })
}

pub async fn upload_image_to_url(
    upload_url: &str,
    image: &Bytes,
    content_type: &str,
    file_name: &str,
) -> Result<()> {
    let client = Client::new();
    let part = reqwest::multipart::Part::bytes(image.to_vec())
        .file_name(file_name.split('/').last().unwrap_or(file_name).to_string())
        .mime_str(content_type)
        .expect("Invalid MIME type");

    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("fileName", file_name.to_string());

    // POST 요청 전송
    let response = client
        .post(upload_url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| {
            error!("Failed to upload file: {}", e);
            anyhow::anyhow!("Failed to upload file: {}", e)
        })?;

    // 응답 확인
    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        error!("File upload failed: {} - {}", status, text);
        return Err(anyhow::anyhow!("File upload failed: {} - {}", status, text));
    }

    info!("File uploaded successfully");
    Ok(())
}

pub async fn request_update_metadata_url(
    file_name: &str,
    metadata: &BotMetadataRequest,
) -> Result<Value> {
    let client = Client::new();
    let worker_api_url = env::var("WORKER_API_URL").expect("WORKER_API_URL not found");
    let update_metadata_url = format!("{}/get-metadata-upload-url", worker_api_url);

    let response = client
        .post(&update_metadata_url)
        .json(&json!({
            "fileName": file_name,
             "metadata":metadata
        }))
        .send()
        .await
        .map_err(|e| {
            error!("Failed to get metadata upload URL: {}", e);
            anyhow::anyhow!("Failed to get metadata upload URL: {}", e)
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        error!("Worker API responded with error: {} - {}", status, text);
        return Err(anyhow::anyhow!(
            "Failed to get metadata upload URL: {} - {}",
            status,
            text
        ));
    }

    info!("Metadata upload URL requested successfully");
    response.json().await.map_err(|e| {
        error!("Failed to parse metadata upload URL response: {}", e);
        anyhow::anyhow!("Failed to parse metadata upload URL response: {}", e)
    })
}

pub async fn upload_metadata_to_url(
    upload_url: &str,
    metadata: BotMetadataRequest,
    file_name: &str,
) -> Result<()> {
    let client = Client::new();
    info!(
        "Uploading metadata to URL: {} \n metadata: {:#?}",
        upload_url, metadata
    );

    let form = reqwest::multipart::Form::new()
        .text("fileName", file_name.to_string())
        .part(
            "file",
            reqwest::multipart::Part::bytes(serde_json::to_vec(&metadata)?)
                .mime_str("application/json")?
                .file_name(file_name.to_string()),
        );

    let response = client
        .post(upload_url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| {
            error!("Failed to upload metadata: {}", e);
            anyhow::anyhow!("Failed to upload metadata: {}", e)
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        error!("Metadata upload failed: {} - {}", status, text);
        return Err(anyhow::anyhow!(
            "Failed to upload metadata: {} - {}",
            status,
            text
        ));
    }

    info!("Metadata uploaded successfully");
    Ok(())
}

pub fn parse_upload_response(response: &Value) -> Result<(&str, String), anyhow::Error> {
    let upload_url = response["url"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing upload URL in response"))?;
    info!("Upload url: {}", upload_url);
    let file_url = response["fileUrl"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing file URL in response"))?;
    info!("File url: {}", file_url);
    Ok((upload_url, file_url.to_string()))
}

pub fn validate_content_type(content_type: &str) -> Option<String> {
    // 이미 유효한 MIME 타입 형식인지 확인 (예: "image/png")
    if content_type.contains("/") && !content_type.contains("://") {
        return Some(content_type.to_string());
    }

    // 파일 확장자를 기반으로 MIME 타입 추론
    if content_type.ends_with(".png") {
        Some("image/png".to_string())
    } else if content_type.ends_with(".jpg") || content_type.ends_with(".jpeg") {
        Some("image/jpeg".to_string())
    } else if content_type.ends_with(".gif") {
        Some("image/gif".to_string())
    } else if content_type.ends_with(".svg") {
        Some("image/svg+xml".to_string())
    } else if content_type.ends_with(".webp") {
        Some("image/webp".to_string())
    } else if content_type.ends_with(".pdf") {
        Some("application/pdf".to_string())
    } else if content_type.ends_with(".json") {
        Some("application/json".to_string())
    } else if content_type.ends_with(".txt") {
        Some("text/plain".to_string())
    } else {
        // URL에서 파일 확장자 추출 시도
        let parts: Vec<&str> = content_type.split(".").collect();
        if parts.len() > 1 {
            let ext = parts.last().unwrap().to_lowercase();
            match ext.as_str() {
                "png" => Some("image/png".to_string()),
                "jpg" | "jpeg" => Some("image/jpeg".to_string()),
                "gif" => Some("image/gif".to_string()),
                "svg" => Some("image/svg+xml".to_string()),
                "webp" => Some("image/webp".to_string()),
                "pdf" => Some("application/pdf".to_string()),
                "json" => Some("application/json".to_string()),
                "txt" => Some("text/plain".to_string()),
                _ => None,
            }
        } else {
            None
        }
    }
}
