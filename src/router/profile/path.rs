#[derive(Debug)]
pub enum ProfilePath {
    GetProfile,
    GetHoldToken,
    GetTokenCreated,
    GetSwapHistory,
    GetPointHistory,
}

impl ProfilePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProfilePath::GetProfile => "/profile/:account_id",
            ProfilePath::GetHoldToken => "/profile/hold-token/:account_id",
            ProfilePath::GetTokenCreated => "/profile/tokens/created/:account_id",
            ProfilePath::GetSwapHistory => "/profile/swap-history/:account_id",
            ProfilePath::GetPointHistory => "/profile/point-history/:account_id",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            ProfilePath::GetProfile => "/profile/{account_id}",
            ProfilePath::GetHoldToken => "/profile/hold-token/{account_id}",
            ProfilePath::GetTokenCreated => "/profile/tokens/created/{account_id}",
            ProfilePath::GetSwapHistory => "/profile/swap-history/{account_id}",
            ProfilePath::GetPointHistory => "/profile/point-history/{account_id}",
        }
    }
}
