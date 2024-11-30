#[derive(Debug)]
pub enum TokenPath {
    GetToken,
    UpdateToken,
}

impl TokenPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenPath::GetToken => "/token/:token",
            TokenPath::UpdateToken => "/token/update",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            TokenPath::GetToken => "/token/{token}",
            TokenPath::UpdateToken => "/token/update",
        }
    }
}
