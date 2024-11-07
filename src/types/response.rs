use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

pub enum Identifier {
    Nickname(String),
    Address(String), // 이더리움 주소
}

#[derive(Serialize, FromRow, ToSchema)]
pub struct HoldTokenResponse {
    pub token_id: String,
    pub amount: Option<String>,
    pub image_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct UserInfoResponse {
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
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: String,
    pub created_at: i64,
    pub user_info: UserInfoResponse, //token 의 creator -> account table -> select nickname, image uri
    pub reply_count: String,         //token.id -> token_reply_count table -> reply_count
    pub price: String,               // token.id -> curve table -> price
}

#[derive(sqlx::FromRow, Debug)]
pub struct SearchTokenRow {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: String,
    pub created_at: i64,
    pub reply_count: String,
    pub price: String,
    // JSON 필드들을 개별적으로 받음
    pub user_nickname: String,
    pub user_account_id: String,
    pub user_image_uri: String,
}
