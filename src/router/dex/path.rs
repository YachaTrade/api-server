#[derive(Debug)]
pub enum DexPath {
    GetPositions,
    GetPool,
    GetTokens,
    GetReserves,
}

impl DexPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            DexPath::GetPositions => "/dex/positions/:account_id",
            DexPath::GetPool => "/dex/pools/:pool_id",
            DexPath::GetTokens => "/dex/tokens",
            DexPath::GetReserves => "/dex/reserves",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            DexPath::GetPositions => "/dex/positions/{account_id}",
            DexPath::GetPool => "/dex/pools/{pool_id}",
            DexPath::GetTokens => "/dex/tokens",
            DexPath::GetReserves => "/dex/reserves",
        }
    }
}
