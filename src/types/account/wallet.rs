use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum Wallet {
    METAMASK,
    KEPLR,
    BACKPACK,
    HAHA,
    PHANTOM,
    RABBY,
    OKX,
    OTHER,
}

impl Wallet {
    pub fn to_string(&self) -> String {
        match self {
            Wallet::METAMASK => "METAMASK".to_string(),
            Wallet::KEPLR => "KEPLR".to_string(),
            Wallet::BACKPACK => "BACKPACK".to_string(),
            Wallet::HAHA => "HAHA".to_string(),
            Wallet::PHANTOM => "PHANTOM".to_string(),
            Wallet::RABBY => "RABBY".to_string(),
            Wallet::OKX => "OKX".to_string(),
            Wallet::OTHER => "OTHER".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterWalletRequest {
    pub wallet: Wallet,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AccountWalletResponse {
    pub account_id: String,
    pub wallet: Wallet,
}
