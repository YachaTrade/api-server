#[derive(Debug)]
pub enum DividendPath {
    /// ② Trade Dividend holder ranking.
    GetHolders,
    /// ① Profile Dividend list.
    GetProfile,
    /// Dividend-token search (candidate payout tokens).
    GetTokens,
}

impl DividendPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            DividendPath::GetHolders => "/trade/dividend/:token_id",
            DividendPath::GetProfile => "/profile/dividend/:account_id",
            DividendPath::GetTokens => "/dividend/tokens",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            DividendPath::GetHolders => "/trade/dividend/{token_id}",
            DividendPath::GetProfile => "/profile/dividend/{account_id}",
            DividendPath::GetTokens => "/dividend/tokens",
        }
    }
}
