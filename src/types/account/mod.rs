use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::common::info::AccountInfo;

// ==================== Validation Constants ====================
pub const NICKNAME_MIN_LENGTH: usize = 1;
pub const NICKNAME_MAX_LENGTH: usize = 15;
pub const MAX_BIO_LENGTH: usize = 200;
pub const MAX_IMAGE_URI_LENGTH: usize = 500;
pub const MAX_X_HANDLE_LENGTH: usize = 100;

// ==================== Common Response ====================

#[derive(Debug, Serialize, ToSchema)]
pub struct AccountResponse {
    pub account_info: AccountInfo,
}

// ==================== Requests ====================

#[derive(Debug, Deserialize, ToSchema)]
pub struct ConnectXRequest {
    pub is_blue_label: bool,
    pub x_handle: String,
    pub x_image_uri: String,
}

impl ConnectXRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.x_handle.len() > MAX_X_HANDLE_LENGTH {
            return Err(format!(
                "x_handle must be at most {} characters",
                MAX_X_HANDLE_LENGTH
            ));
        }
        if self.x_image_uri.len() > MAX_IMAGE_URI_LENGTH {
            return Err(format!(
                "x_image_uri must be at most {} characters",
                MAX_IMAGE_URI_LENGTH
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterWalletRequest {
    pub wallet: String,
}

impl RegisterWalletRequest {
    const ALLOWED_WALLETS: &'static [&'static str] = &[
        "METAMASK", "KEPLR", "BACKPACK", "HAHA", "OKX", "PHANTOM", "RABBY", "OTHER",
    ];

    pub fn validate(&self) -> Result<(), String> {
        if !Self::ALLOWED_WALLETS.contains(&self.wallet.as_str()) {
            return Err(format!(
                "Invalid wallet type. Allowed: {:?}",
                Self::ALLOWED_WALLETS
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateAccountRequest {
    #[serde(default)]
    pub bio: Option<String>,
    #[serde(default)]
    pub image_uri: Option<String>,
    #[serde(default)]
    pub nickname: Option<String>,
}

impl UpdateAccountRequest {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(ref bio) = self.bio
            && bio.len() > MAX_BIO_LENGTH
        {
            return Err(format!("Bio must be at most {} characters", MAX_BIO_LENGTH));
        }
        if let Some(ref nickname) = self.nickname {
            let char_count = nickname.chars().count();
            if char_count < NICKNAME_MIN_LENGTH {
                return Err(format!(
                    "Nickname must be at least {} character",
                    NICKNAME_MIN_LENGTH
                ));
            }
            if char_count > NICKNAME_MAX_LENGTH {
                return Err(format!(
                    "Nickname must be at most {} characters",
                    NICKNAME_MAX_LENGTH
                ));
            }
            if nickname.starts_with('@') || nickname.starts_with('#') {
                return Err("Nickname cannot start with @ or #".to_string());
            }
        }
        if let Some(ref image_uri) = self.image_uri
            && image_uri.len() > MAX_IMAGE_URI_LENGTH
        {
            return Err(format!(
                "image_uri must be at most {} characters",
                MAX_IMAGE_URI_LENGTH
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateXRequest {
    pub x_image_uri: String,
}

impl UpdateXRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.x_image_uri.len() > MAX_IMAGE_URI_LENGTH {
            return Err(format!(
                "x_image_uri must be at most {} characters",
                MAX_IMAGE_URI_LENGTH
            ));
        }
        Ok(())
    }
}

// ==================== Upload Image Response ====================

#[derive(Debug, Serialize, ToSchema)]
pub struct UploadImageResponse {
    pub image_uri: String,
}

// ==================== GET /account/wallet ====================

#[derive(Debug, Serialize, ToSchema)]
pub struct GetWalletResponse {
    pub account_id: String,
    pub wallet: String,
}
