use crate::env;

use anyhow::Result;
use aws_config::Region;
use aws_sdk_s3::{config::Credentials, primitives::ByteStream, Client};
use bytes::Bytes;
use tracing::info;

#[derive(Debug)]
pub struct S3Client {
    client: Client,
    bucket: String,
}

impl S3Client {
    pub async fn new() -> Self {
        let bucket = env::get_env("AWS_BUCKET_NAME");
        let access_key = env::get_env("AWS_ACCESS_KEY");
        let secret_access_key = env::get_env("AWS_SECRET_ACCESS_KEY");
        let credentials = Credentials::new(access_key, secret_access_key, None, None, "aws-s3");

        let config = aws_config::from_env()
            .credentials_provider(credentials)
            .region(Region::new("ap-northeast-2"))
            .load()
            .await;

        let client = Client::new(&config);

        S3Client {
            client,
            bucket: bucket.to_string(),
        }
    }
    /*
    TODO :else 일 경우 도메인 결정
    */
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
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(body))
            .content_type(content_type)
            .send()
            .await?;
        info!("Uploaded Result ={:?}", result);
        info!("Uploaded file to S3: key={}", key);

        Ok(self.get_presigned_url(&key).await?)
    }
    pub async fn upload_profile_image_file<'a>(
        &self,
        account_id: &str,
        body: Bytes,
        content_type: &str,
    ) -> Result<String> {
        let key = format!("profile/{account_id}");
        let result = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(body))
            .content_type(content_type)
            .send()
            .await?;
        info!("Uploaded Result ={:?}", result);
        info!("Uploaded file to S3: key={}", key);

        Ok(self.get_presigned_url(&key).await?)
    }
    pub async fn get_file(&self, key: &str) -> Result<Bytes> {
        let result = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        Ok(result.body.collect().await?.into_bytes())
    }

    pub async fn delete_file(&self, key: &str) -> Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await?;

        Ok(())
    }

    pub async fn get_presigned_url(&self, key: &str) -> Result<String> {
        let presigned_request = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .presigned(aws_sdk_s3::presigning::PresigningConfig::expires_in(
                std::time::Duration::from_secs(3600),
            )?)
            .await?;

        Ok(presigned_request.uri().to_string())
    }
}
