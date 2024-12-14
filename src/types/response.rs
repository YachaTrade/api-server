use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug)]
pub enum Identifier {
    Nickname(String),
    Address(String), // 이더리움 주소
}

#[derive(Serialize, FromRow, ToSchema)]
pub struct HoldTokenResponse {
    pub token_id: String,
    pub symbol: String,
    pub price: String,
    pub amount: String,
    pub image_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct AccountInfo {
    pub nickname: String,
    pub account_id: String,
    pub image_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct TokenInfoResponse {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct SearchTokenResponse {
    pub token_info: SearchTokenInfo,
    pub account_info: AccountInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SearchTokenInfo {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: String,
    pub reply_count: String,
    pub price: String, //curve.price
    pub reserve_token: String,
    pub created_at: i64,
    pub is_king: bool,
    pub score: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SearchTokenRaw {
    pub token_id: String,
    pub account_id: String,
    pub nickname: String,
    pub account_image_uri: String,
    pub name: String,
    pub symbol: String,
    pub token_image_uri: String,
    pub description: Option<String>,
    pub reply_count: String,
    pub price: String,
    pub reserve_token: String,
    pub is_king: bool,
    pub created_at: i64,
    pub score: f64,
}

impl From<SearchTokenRaw> for SearchTokenResponse {
    fn from(row: SearchTokenRaw) -> Self {
        SearchTokenResponse {
            token_info: SearchTokenInfo {
                token_id: row.token_id,
                name: row.name,
                symbol: row.symbol,
                image_uri: row.token_image_uri,
                description: row.description.unwrap_or_default(),
                reply_count: row.reply_count,
                price: row.price,
                reserve_token: row.reserve_token,
                created_at: row.created_at,
                is_king: row.is_king,
                score: row.score,
            },
            account_info: AccountInfo {
                account_id: row.account_id,
                image_uri: row.account_image_uri,
                nickname: row.nickname,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct MintPartyResponse {
    pub account_info: AccountInfo,
    pub mint_party_id: String,
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub image_uri: String,
    pub current_white_list_count: String,
    pub allow_white_list_count: String,
    pub funding_amount: String,
    pub total_deposit_amount: String,
}
