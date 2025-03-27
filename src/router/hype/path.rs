#[derive(Debug)]
pub enum HypePath {
    GetHype,
    GetHonor,
}

impl HypePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            HypePath::GetHype => "/hype_token",
            HypePath::GetHonor => "/honor_token",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            HypePath::GetHype => "/hype_token",
            HypePath::GetHonor => "/honor_token",
        }
    }
}
