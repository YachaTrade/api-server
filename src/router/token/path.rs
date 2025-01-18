#[derive(Debug)]
pub enum TokenPath {
    GetToken,
}

impl TokenPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            TokenPath::GetToken => "/token/:token",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            TokenPath::GetToken => "/token/{token}",
        }
    }
}
