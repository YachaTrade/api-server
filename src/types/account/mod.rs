use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::common::info::AccountInfo;

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

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterWalletRequest {
    pub wallet: String,
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateXRequest {
    pub x_image_uri: String,
}

// ==================== GET /account/wallet ====================

#[derive(Debug, Serialize, ToSchema)]
pub struct GetWalletResponse {
    pub account_id: String,
    pub wallet: String,
}
