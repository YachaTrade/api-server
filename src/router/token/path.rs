#[derive(Debug)]
pub enum TokenPath {
    GetToken,
    GetMetadata,
    Salt,
    Hackathon,
}

impl TokenPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenPath::GetToken => "/token/:token",
            TokenPath::GetMetadata => "/token/metadata/:token_id",
            TokenPath::Salt => "/token/salt",
            TokenPath::Hackathon => "/token/hackathon",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            TokenPath::GetToken => "/token/{token}",
            TokenPath::GetMetadata => "/token/metadata/{token}",
            TokenPath::Salt => "/token/salt",
            TokenPath::Hackathon => "/token/hackathon",
        }
    }
}
