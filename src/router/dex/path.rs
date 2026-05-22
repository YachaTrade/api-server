#[derive(Debug)]
pub enum DexPath {
    GetPositions,
    GetPool,
    GetTokens,
    SearchTokens,
}

impl DexPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            DexPath::GetPositions => "/dex/positions/:account_id",
            DexPath::GetPool => "/dex/pools/:pool_id",
            DexPath::GetTokens => "/dex/tokens",
            DexPath::SearchTokens => "/dex/search",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            DexPath::GetPositions => "/dex/positions/{account_id}",
            DexPath::GetPool => "/dex/pools/{pool_id}",
            DexPath::GetTokens => "/dex/tokens",
            DexPath::SearchTokens => "/dex/search",
        }
    }
}
