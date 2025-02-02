pub enum FollowPath {
    AddFollow,
    RemoveFollow,
    CheckFollow,
    GetFollowers,
    GetFollowings,
}

impl FollowPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            FollowPath::AddFollow => "/follow/add",
            FollowPath::RemoveFollow => "/follow/remove",
            FollowPath::CheckFollow => "/follow/check/:account_id",
            FollowPath::GetFollowers => "/follow/followers/:account_id",
            FollowPath::GetFollowings => "/follow/followings/:account_id",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            FollowPath::AddFollow => "/follow/add",
            FollowPath::RemoveFollow => "/follow/remove",
            FollowPath::CheckFollow => "/follow/check/{account_id}",
            FollowPath::GetFollowers => "/follow/followers/{account_id}",
            FollowPath::GetFollowings => "/follow/followings/{account_id}",
        }
    }
}
