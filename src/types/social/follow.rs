use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::types::common::info::AccountInfo;

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdateFollowRequest {
    pub follower: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct UpdateFollowResponse {
    pub follower: AccountInfo,
    pub following: AccountInfo,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckFollowResponse {
    pub is_following: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FollowersResponse {
    pub accounts: Vec<AccountInfo>,
    pub total_count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FollowingResponse {
    pub accounts: Vec<AccountInfo>,
    pub total_count: i64,
}
