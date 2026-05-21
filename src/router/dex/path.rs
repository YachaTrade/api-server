#[derive(Debug)]
pub enum DexPath {
    GetPositions,
}

impl DexPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            DexPath::GetPositions => "/dex/positions/:account_id",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            DexPath::GetPositions => "/dex/positions/{account_id}",
        }
    }
}
