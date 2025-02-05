#[derive(Debug)]
pub enum ProfilePath {
    GetProfile,
    GetPnl,
    GetPosition,
    GetTokenCreated,
    GetSwapHistory,
}

impl ProfilePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProfilePath::GetProfile => "/profile/:account_id",
            ProfilePath::GetPnl => "/profile/pnl/:account_id",
            ProfilePath::GetPosition => "/profile/position/:account_id",
            ProfilePath::GetTokenCreated => "/profile/tokens/created/:account_id",
            ProfilePath::GetSwapHistory => "/profile/swap-history/:account_id",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            ProfilePath::GetProfile => "/profile/{account_id}",
            ProfilePath::GetPnl => "/profile/pnl/{account_id}",
            ProfilePath::GetPosition => "/profile/position/{account_id}",
            ProfilePath::GetTokenCreated => "/profile/tokens/created/{account_id}",
            ProfilePath::GetSwapHistory => "/profile/swap-history/{account_id}",
        }
    }
}
