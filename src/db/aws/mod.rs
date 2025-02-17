use std::env;

use anyhow::{anyhow, Result};
use aws_config::Region;
use aws_sdk_cloudfront::Client as CloudFrontClient;

use aws_sdk_s3::{config::Credentials, primitives::ByteStream, Client};
use bytes::Bytes;
use chrono;
use tracing::{error, info};

// S3Client struct for managing AWS S3 and CloudFront operations
// Handles file uploads, downloads, and CDN cache invalidation
#[derive(Debug)]
pub struct S3Client {
    client: Client,                    // AWS S3 client
    cloudfront_client: CloudFrontClient, // AWS CloudFront client
    bucket_name: String,               // Target S3 bucket name
    distribution_id: String,           // CloudFront distribution ID
}

// CloudFront CDN domain constant
const CDN_DOMAIN: &str = "d1469zz8b08zl.cloudfront.net";

impl S3Client {
    // Creates a new S3Client instance with AWS credentials from environment variables
    // Initializes both S3 and CloudFront clients
    pub async fn new() -> Self {
        let bucket_name = env::var("AWS_BUCKET_NAME").expect("AWS_BUCKET_NAME must be set");
        info!("S3 bucket name from env: {}", bucket_name);
        let access_key = env::var("AWS_ACCESS_KEY").expect("AWS_ACCESS_KEY must be set");
        let secret_access_key =
            env::var("AWS_SECRET_ACCESS_KEY").expect("AWS_SECRET_ACCESS_KEY must be set");
        let distribution_id = env::var("AWS_CLOUDFRONT_DISTRIBUTION_ID")
            .expect("AWS_CLOUDFRONT_DISTRIBUTION_ID must be set");
        let credentials = Credentials::new(access_key, secret_access_key, None, None, "aws-s3");
        let region = Region::new("ap-northeast-1");

        let config = aws_config::from_env()
            .credentials_provider(credentials.clone())
            .region(region.clone())
            .load()
            .await;

        let client = Client::new(&config);
        let cloudfront_client = CloudFrontClient::new(&config);

        S3Client {
            client,
            cloudfront_client,
            bucket_name: bucket_name.to_string(),
            distribution_id: distribution_id.to_string(),
        }
    }

    // Invalidates CloudFront CDN cache for a given key
    // This ensures that updated content is immediately available through the CDN
    async fn invalidate_cdn_cache(&self, key: &str) -> Result<()> {
        let path = format!("/{}", key);

        let paths = aws_sdk_cloudfront::types::Paths::builder()
            .quantity(1)
            .items(path)
            .build()
            .map_err(|e| anyhow!("Failed to build paths: {}", e))?;

        let batch = aws_sdk_cloudfront::types::InvalidationBatch::builder()
            .caller_reference(&format!("{}-{}", key, chrono::Utc::now().timestamp()))
            .paths(paths)
            .build()
            .map_err(|e| anyhow!("Failed to build invalidation batch: {}", e))?;

        match self
            .cloudfront_client
            .create_invalidation()
            .distribution_id(&self.distribution_id)
            .invalidation_batch(batch)
            .send()
            .await
        {
            Ok(_) => {
                info!("Successfully invalidated CDN cache for key: {}", key);
                Ok(())
            }
            Err(err) => {
                error!(
                    "Failed to invalidate CDN cache for key: {}, error: {:?}",
                    key, err
                );
                Err(anyhow!("CDN cache invalidation failed. Error: {}", err))
            }
        }
    }

    // Uploads a thread image file to S3 and returns the CDN URL
    // Parameters:
    // - account_id: User's account ID
    // - thread_id: Thread identifier
    // - body: Image file contents
    // - content_type: MIME type of the image
    pub async fn upload_thread_image_file<'a>(
        &self,
        account_id: &str,
        thread_id: i32,
        body: Bytes,
        content_type: &str,
    ) -> Result<String> {
        let key = format!("thread/{thread_id}/{account_id}");
        let result = self
            .client
            .put_object()
            .bucket(&self.bucket_name)
            .key(&key)
            .body(ByteStream::from(body))
            .content_type(content_type)
            .send()
            .await;
        match result {
            Ok(output) => {
                info!(
                    "Successfully uploaded thread image to S3: key={}, output={:?}",
                    key, output
                );

                // Invalidate CDN cache after successful upload
                if let Err(err) = self.invalidate_cdn_cache(&key).await {
                    error!(
                        "CDN cache invalidation failed but upload succeeded: {}",
                        err
                    );
                }

                let cdn_url = format!("https://{}/{}", CDN_DOMAIN, key);
                Ok(cdn_url)
            }
            Err(err) => {
                error!(
                    "Failed to upload thread image to S3: key={}, error={:?}",
                    key, err
                );
                error!("Error details: {:?}", err.to_string());
                Err(anyhow!("Update thread image failed. Error: {}", err))
            }
        }
    }

    // Uploads a profile image file to S3 and returns the CDN URL
    // Parameters:
    // - account_id: User's account ID
    // - body: Image file contents
    // - content_type: MIME type of the image
    pub async fn upload_profile_image_file(
        &self,
        account_id: &str,
        body: Bytes,
        content_type: String,
    ) -> Result<String> {
        let key = format!("profile/{}", account_id);
        info!(
            "Uploading profile image to S3: key={}, content_type={}",
            key, content_type
        );

        let result = self
            .client
            .put_object()
            .bucket(&self.bucket_name)
            .key(&key)
            .body(ByteStream::from(body))
            .content_type(content_type)
            .send()
            .await;

        match result {
            Ok(output) => {
                info!(
                    "Successfully uploaded profile image to S3: key={}, output={:?}",
                    key, output
                );

                // Invalidate CDN cache after successful upload
                if let Err(err) = self.invalidate_cdn_cache(&key).await {
                    error!(
                        "CDN cache invalidation failed but upload succeeded: {}",
                        err
                    );
                }

                let cdn_url = format!("https://{}/{}", CDN_DOMAIN, key);
                Ok(cdn_url)
            }
            Err(err) => {
                error!(
                    "Failed to upload profile image to S3: key={}, error={:?}",
                    key, err
                );
                error!("Error details: {:?}", err.to_string());
                Err(anyhow!("Update profile image failed. Error: {}", err))
            }
        }
    }

    // Retrieves a file from S3 by its key
    // Returns the file contents as Bytes
    pub async fn get_file(&self, key: &str) -> Result<Bytes> {
        let result = self
            .client
            .get_object()
            .bucket(&self.bucket_name)
            .key(key)
            .send()
            .await?;

        Ok(result.body.collect().await?.into_bytes())
    }

    // Deletes a file from S3 by its key
    pub async fn delete_file(&self, key: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket_name)
            .key(key)
            .send()
            .await?;

        Ok(())
    }

    // Generates a pre-signed URL for temporary access to a private S3 object
    // The URL expires after 1 hour (3600 seconds)
    pub async fn get_presigned_url(&self, key: &str) -> Result<String> {
        let presigned_request = self
            .client
            .get_object()
            .bucket(&self.bucket_name)
            .key(key)
            .presigned(aws_sdk_s3::presigning::PresigningConfig::expires_in(
                std::time::Duration::from_secs(3600),
            )?)
            .await?;

        Ok(presigned_request.uri().to_string())
    }
}
