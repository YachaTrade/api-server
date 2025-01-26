pub enum FollowPath {
    AddFollow,
    RemoveFollow,
    GetFollowers,
    GetFollowings,
}

impl FollowPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            FollowPath::AddFollow => "/follow/add",
            FollowPath::RemoveFollow => "/follow/remove",
            FollowPath::GetFollowers => "/follow/:account_id/followers",
            FollowPath::GetFollowings => "/follow/:account_id/followings",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            FollowPath::AddFollow => "/follow/add",
            FollowPath::RemoveFollow => "/follow/remove",
            FollowPath::GetFollowers => "/follow/{account_id}/followers",
            FollowPath::GetFollowings => "/follow/{account_id}/followings",
        }
    }
}
