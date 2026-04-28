#[derive(Debug)]
pub enum ProfilePath {
    GetProfile,
    GetHoldToken,
    GetTokenCreated,
    GetGiftFee,
    GetSwapHistory,
    GetPointHistory,
}

impl ProfilePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProfilePath::GetProfile => "/profile/:account_id",
            ProfilePath::GetHoldToken => "/profile/hold-token/:account_id",
            ProfilePath::GetTokenCreated => "/profile/tokens/created/:account_id",
            ProfilePath::GetGiftFee => "/profile/gift-fee/:account_id",
            ProfilePath::GetSwapHistory => "/profile/swap-history/:account_id",
            ProfilePath::GetPointHistory => "/profile/point-history",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            ProfilePath::GetProfile => "/profile/{account_id}",
            ProfilePath::GetHoldToken => "/profile/hold-token/{account_id}",
            ProfilePath::GetTokenCreated => "/profile/tokens/created/{account_id}",
            ProfilePath::GetGiftFee => "/profile/gift-fee/{account_id}",
            ProfilePath::GetSwapHistory => "/profile/swap-history/{account_id}",
            ProfilePath::GetPointHistory => "/profile/point-history",
        }
    }
}
