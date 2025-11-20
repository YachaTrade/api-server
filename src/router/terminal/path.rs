#[derive(Debug)]
pub enum TerminalPath {
    LatestBlock,
    Asset,
    Pair,
    Events,
    GetMetadata,
}

impl TerminalPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            TerminalPath::LatestBlock => "/latest-block",
            TerminalPath::Asset => "/asset",
            TerminalPath::Pair => "/pair",
            TerminalPath::Events => "/events",
            TerminalPath::GetMetadata => "/:token_address",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            TerminalPath::LatestBlock => "/latest-block",
            TerminalPath::Asset => "/asset",
            TerminalPath::Pair => "/pair",
            TerminalPath::Events => "/events",
            TerminalPath::GetMetadata => "/{token_address}",
        }
    }
}
