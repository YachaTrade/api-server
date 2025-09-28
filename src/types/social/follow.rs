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
pub struct FollowsResponse {
    pub accounts: Vec<Follow>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct FollowResponse {
    pub follower: AccountInfo,
    pub following: AccountInfo,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct Follow {
    pub account: AccountInfo,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct CheckFollowResponse {
    pub is_following: bool,
}
