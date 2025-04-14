#[derive(Debug)]
pub enum TokenPath {
    GetToken,
    GetMetadata,
}

impl TokenPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenPath::GetToken => "/token/:token",
            TokenPath::GetMetadata => "/token/metadata/:token",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            TokenPath::GetToken => "/token/{token}",
            TokenPath::GetMetadata => "/token/metadata/{token}",
        }
    }
}
