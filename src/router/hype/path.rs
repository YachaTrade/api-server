#[derive(Debug)]
pub enum HypePath {
    GetHype,
}

impl HypePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            HypePath::GetHype => "/hype_token",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            HypePath::GetHype => "/hype_token",
        }
    }
}
