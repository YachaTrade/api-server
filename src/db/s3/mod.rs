pub mod test;
use crate::env;

use anyhow::Result;
use aws_config::Region;
use aws_sdk_s3::{config::Credentials, primitives::ByteStream, types::ObjectCannedAcl, Client};
use bytes::Bytes;
use tracing::info;

#[derive(Debug)]
pub struct S3Client {
    client: Client,
    bucket: String,
}

impl S3Client {
    pub async fn new() -> Self {
        let account_id = env::get_env("S3_ACCOUNT_ID");
        let bucket = env::get_env("S3_BUCKET");
        let access_key_id = env::get_env("S3_ACCESS_KEY_ID");
        let secret_access_key = env::get_env("S3_SECRET_ACCESS_KEY");
        let credentials = Credentials::new(
            access_key_id,
            secret_access_key,
            None,
            None,
            "cloudflare-s3",
        );

        let config = aws_config::from_env()
            .credentials_provider(credentials)
            .region(Region::new("auto"))
            .endpoint_url(format!(
                "https://{}.s3.cloudflarestorage.com/{}",
                account_id, bucket
            ))
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
        let key = format!("thread:{thread_id}/account:{account_id}");
        let result = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(body))
            .content_type(content_type)
            .acl(ObjectCannedAcl::PublicRead)
            .send()
            .await?;
        info!("Uploaded Result ={:?}", result);
        info!("Uploaded file to S3: key={}", key);
        let environment = env::get_env("ENVIRONMENT");
        if environment == "development" {
            return Ok(format!(
                "https://pub-56950a3ba13e4c43ba0b2e803fd9b2f1.s3.dev/{}/{}",
                self.bucket, key
            ));
        } else {
            return Ok(format!(
                "https://c7dff00d9c1deefa16a9c134c98dd4a4.s3.cloudflarestorage.com/{}/{}",
                self.bucket, key
            ));
        }
    }
    pub async fn upload_profile_image_file<'a>(
        &self,
        account_id: &str,
        body: Bytes,
        content_type: &str,
    ) -> Result<String> {
        let key = format!("profile:{account_id}");
        let result = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(&key)
            .body(ByteStream::from(body))
            .content_type(content_type)
            .acl(ObjectCannedAcl::PublicRead)
            .send()
            .await?;
        info!("Uploaded Result ={:?}", result);
        info!("Uploaded file to S3: key={}", key);
        let environment = env::get_env("ENVIRONMENT");
        if environment == "development" {
            return Ok(format!(
                "https://pub-56950a3ba13e4c43ba0b2e803fd9b2f1.s3.dev/{}/{}",
                self.bucket, key
            ));
        } else {
            return Ok(format!(
                "https://c7dff00d9c1deefa16a9c134c98dd4a4.s3.cloudflarestorage.com/{}/{}",
                self.bucket, key
            ));
        }
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
}
