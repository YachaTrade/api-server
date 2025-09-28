use crate::types::common::info::XInfo;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConnectXRequest {
    pub x_handle: String,
    pub x_image_uri: String,
    pub is_blue_label: bool,
}
impl ConnectXRequest {
    pub fn validate(&self) -> Result<(), anyhow::Error> {
        // x_handle 검증
        if !self.x_handle.starts_with('@') {
            return Err(anyhow!("X handle must start with @"));
        }

        if self.x_handle.len() > 16 {
            return Err(anyhow!("X handle must be 16 characters or less"));
        }

        // x_image_uri 검증
        if !url::Url::parse(&self.x_image_uri).is_ok() {
            return Err(anyhow!("Invalid image URI format"));
        }

        Ok(())
    }
}
#[derive(Debug, Deserialize, ToSchema)]
pub struct DisconnectXRequest {
    pub x_handle: String,
}

#[derive(Serialize, ToSchema)]
pub struct ConnectedXAccountResponse {
    pub account_id: String,
    pub x_handle: String,
    pub x_image_uri: String,
    pub is_blue_label: bool,
}
#[derive(Serialize, ToSchema)]
pub struct DisconnectedXAccountResponse {
    pub account_id: String,
    pub x_handle: String,
}

#[derive(Serialize, ToSchema)]
pub struct GetXHandleResponse {
    pub account_id: String,
    pub x_info: XInfo,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateXRequest {
    pub x_image_uri: String,
}
impl UpdateXRequest {
    pub fn validate(&self) -> Result<(), anyhow::Error> {
        if !self
            .x_image_uri
            .starts_with("https://pbs.twimg.com/profile_images/")
        {
            return Err(anyhow!(
                "X image URI must start with https://pbs.twimg.com/profile_images/"
            ));
        }

        Ok(())
    }
}
