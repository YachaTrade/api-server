pub mod wallet;
pub mod x;
use std::env;

use rand::Rng;
use serde::{Deserialize, Serialize};

use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct UpdateAccountRequest {
    pub nickname: Option<String>,
    pub bio: Option<String>,
    pub image_uri: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct AccountResponse {
    pub account: Account,
}

#[derive(Deserialize, ToSchema)]
pub struct AccountParams {
    pub account_id: String,
    pub request_account_id: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RequestAccountIdParam {
    pub request_account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct MutualFriend {
    pub account_id: String,
    pub nickname: String,
    pub image_uri: String,
    pub follower_count: i32,
    pub following_count: i32,
}

#[derive(Debug, Clone, Serialize, FromRow, Deserialize, ToSchema)]
pub struct Mutual {
    pub mutual_friends: Option<Vec<MutualFriend>>,
    pub mutual_friends_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct Account {
    pub account_id: String,
    pub nickname: String,
    pub image_uri: String,
    pub bio: String,
    pub follower_count: i32,
    pub following_count: i32,
    pub mutual: Option<Mutual>,
}

impl Account {
    pub fn new(account_id: String) -> Self {
        //random_number 는 1~5까지의 숫자가 나와야함.
        let random_number = rand::thread_rng().gen_range(1..=5);
        let image_key = format!("DEFAULT_IMAGE_{}", random_number);
        let image_uri = env::var(&image_key).expect("DEFAULT_IMAGE must be set");
        Self {
            account_id: account_id.clone(),
            image_uri,
            nickname: account_id,
            bio: "".to_string(),
            follower_count: 0,
            following_count: 0,
            mutual: None,
        }
    }
}
