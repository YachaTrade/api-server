#[derive(Debug)]
pub enum TokenPath {
    GetToken,
    GetMetadata,
    MineSalt,
}

impl TokenPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenPath::GetToken => "/token/:token",
            TokenPath::GetMetadata => "/token/metadata/:token_id",
            TokenPath::MineSalt => "/token/mine-salt",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            TokenPath::GetToken => "/token/{token}",
            TokenPath::GetMetadata => "/token/metadata/{token}",
            TokenPath::MineSalt => "/token/mine-salt",
        }
    }
}
