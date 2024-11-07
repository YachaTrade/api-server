pub mod test;
use crate::env;

use anyhow::Result;
use aws_config::Region;
use aws_sdk_s3::{config::Credentials, primitives::ByteStream, types::ObjectCannedAcl, Client};
use bytes::Bytes;
use tracing::info;

#[derive(Debug)]
pub struct R2Client {
    client: Client,
    bucket: String,
}

impl R2Client {
    pub async fn new() -> Self {
        let account_id = env::get_env("R2_ACCOUNT_ID");
        let bucket = env::get_env("R2_BUCKET");
        let access_key_id = env::get_env("R2_ACCESS_KEY_ID");
        let secret_access_key = env::get_env("R2_SECRET_ACCESS_KEY");
        let credentials = Credentials::new(
            access_key_id,
            secret_access_key,
            None,
            None,
            "cloudflare-r2",
        );

        let config = aws_config::from_env()
            .credentials_provider(credentials)
            .region(Region::new("auto"))
            .endpoint_url(format!(
                "https://{}.r2.cloudflarestorage.com/{}",
                account_id, bucket
            ))
            .load()
            .await;

        let client = Client::new(&config);

        R2Client {
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
        info!("Uploaded file to R2: key={}", key);
        let environment = env::get_env("ENVIRONMENT");
        if environment == "development" {
            return Ok(format!(
                "https://pub-56950a3ba13e4c43ba0b2e803fd9b2f1.r2.dev/{}/{}",
                self.bucket, key
            ));
        } else {
            return Ok(format!(
                "https://c7dff00d9c1deefa16a9c134c98dd4a4.r2.cloudflarestorage.com/{}/{}",
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
        info!("Uploaded file to R2: key={}", key);
        let environment = env::get_env("ENVIRONMENT");
        if environment == "development" {
            return Ok(format!(
                "https://pub-56950a3ba13e4c43ba0b2e803fd9b2f1.r2.dev/{}/{}",
                self.bucket, key
            ));
        } else {
            return Ok(format!(
                "https://c7dff00d9c1deefa16a9c134c98dd4a4.r2.cloudflarestorage.com/{}/{}",
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
