#[derive(Debug)]
pub enum GeckoPath {
    LatestBlock,
    Asset,
    Pair,
    Events,
    GetMetadata,
}

impl GeckoPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            GeckoPath::LatestBlock => "/latest-block",
            GeckoPath::Asset => "/asset",
            GeckoPath::Pair => "/pair",
            GeckoPath::Events => "/events",
            GeckoPath::GetMetadata => "/:token_address",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            GeckoPath::LatestBlock => "/latest-block",
            GeckoPath::Asset => "/asset",
            GeckoPath::Pair => "/pair",
            GeckoPath::Events => "/events",
            GeckoPath::GetMetadata => "/{token_address}",
        }
    }
}
