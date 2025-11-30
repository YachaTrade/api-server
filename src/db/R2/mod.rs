use std::env;
use std::time::Instant;

use anyhow::{Result, anyhow};
use aws_config::{BehaviorVersion, Region};

use aws_sdk_s3::{Client, config::Credentials, primitives::ByteStream};
use bytes::Bytes;

use tracing::{error, info};

// R2Client struct for managing AWS R2 and CloudFront operations
// Handles file uploads, downloads, and CDN cache invalidation
#[derive(Debug)]
pub struct R2Client {
    client: Client,      // AWS R2 client
    bucket_name: String, // Target R2 bucket name
}

impl R2Client {
    // Creates a new R2Client instance with Cloudflare R2 credentials
    pub async fn new() -> Self {
        let bucket_name = env::var("R2_BUCKET_NAME").expect("R2_BUCKET_NAME must be set");
        info!("R2 bucket name from env: {}", bucket_name);
        let access_key = env::var("R2_ACCESS_KEY").expect("R2_ACCESS_KEY must be set");
        let secret_access_key =
            env::var("R2_SECRET_ACCESS_KEY").expect("R2_SECRET_ACCESS_KEY must be set");
        let account_id =
            env::var("CLOUDFLARE_ACCOUNT_ID").expect("CLOUDFLARE_ACCOUNT_ID must be set");
        let credentials = Credentials::new(access_key, secret_access_key, None, None, "r2");

        // R2 엔드포인트
        let r2_endpoint = format!("https://{}.r2.cloudflarestorage.com", account_id);

        let config = aws_config::defaults(BehaviorVersion::latest())
            .credentials_provider(credentials.clone())
            .endpoint_url(r2_endpoint)
            .region(Region::new("auto"))
            .load()
            .await;

        let client = Client::new(&config);

        R2Client {
            client,
            bucket_name: bucket_name.to_string(),
        }
    }

    // Uploads a metadata image file to R2 and returns the CDN URL
    // Parameters:
    // - image_id: Unique identifier for the image
    // - body: Image file contents
    // - content_type: MIME type of the image
    pub async fn upload_metadata_image_file(
        &self,
        image_id: &str,
        body: &Bytes,
        content_type: &str,
    ) -> Result<String> {
        let key = format!("coin/{}", image_id);
        info!(
            "Uploading metadata image to R2: key={}, content_type={}",
            key, content_type
        );

        let result = self
            .client
            .put_object()
            .bucket(&self.bucket_name)
            .key(&key)
            .body(ByteStream::from(body.clone()))
            .content_type(content_type)
            .send()
            .await;

        match result {
            Ok(output) => {
                info!(
                    "Successfully uploaded metadata image to R2: key={}, output={:?}",
                    key, output
                );

                // R2 Custom Domain URL
                let r2_url = format!("https://storage.nadapp.net/{}", key);
                Ok(r2_url)
            }
            Err(err) => {
                error!(
                    "Failed to upload metadata image to R2: key={}, error={:?}",
                    key, err
                );
                Err(anyhow!("Upload metadata image failed. Error: {}", err))
            }
        }
    }

    // Uploads metadata JSON file to R2 and returns the CDN URL
    // Parameters:
    // - metadata_id: Unique identifier for the metadata
    // - metadata: TokenMetadata struct to be serialized as JSON
    pub async fn upload_metadata_file(
        &self,
        metadata_id: &str,
        metadata: &crate::types::metadata::TokenMetadata,
    ) -> Result<String> {
        let start_time = Instant::now();
        let key = format!("metadata/{}.json", metadata_id);
        info!(
            "Uploading metadata to R2: key={}, name={}, symbol={}",
            key, metadata.name, metadata.symbol
        );

        // Serialize metadata to JSON
        let json_data = serde_json::to_string_pretty(metadata)
            .map_err(|e| anyhow!("Failed to serialize metadata: {}", e))?;

        let result = self
            .client
            .put_object()
            .bucket(&self.bucket_name)
            .key(&key)
            .body(ByteStream::from(json_data.into_bytes()))
            .content_type("application/json")
            .send()
            .await;

        let elapsed = start_time.elapsed();

        match result {
            Ok(output) => {
                info!(
                    "Successfully uploaded metadata to R2: key={}, elapsed={:?}, output={:?}",
                    key, elapsed, output
                );

                // R2 Custom Domain URL
                let r2_url = format!("https://storage.nadapp.net/{}", key);
                Ok(r2_url)
            }
            Err(err) => {
                error!(
                    "Failed to upload metadata to R2: key={}, elapsed={:?}, error={:?}",
                    key, elapsed, err
                );
                Err(anyhow!("Upload metadata failed. Error: {}", err))
            }
        }
    }

    // Uploads account profile image to R2 and returns the CDN URL
    // Parameters:
    // - image_id: Unique identifier (UUID) for the image
    // - body: Image file contents
    // - content_type: MIME type of the image
    pub async fn upload_account_image_file(
        &self,
        image_id: &str,
        body: &Bytes,
        content_type: &str,
    ) -> Result<String> {
        let key = format!("account/{}", image_id);
        info!(
            "Uploading account image to R2: key={}, content_type={}",
            key, content_type
        );

        let result = self
            .client
            .put_object()
            .bucket(&self.bucket_name)
            .key(&key)
            .body(ByteStream::from(body.clone()))
            .content_type(content_type)
            .send()
            .await;

        match result {
            Ok(output) => {
                info!(
                    "Successfully uploaded account image to R2: key={}, output={:?}",
                    key, output
                );

                // R2 Custom Domain URL
                let r2_url = format!("https://storage.nadapp.net/{}", key);
                Ok(r2_url)
            }
            Err(err) => {
                error!(
                    "Failed to upload account image to R2: key={}, error={:?}",
                    key, err
                );
                Err(anyhow!("Upload account image failed. Error: {}", err))
            }
        }
    }
}
