pub enum ActiveUserPath {
    CheckActiveUser,
}

impl ActiveUserPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CheckActiveUser => "/usage/:wallet_address",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            Self::CheckActiveUser => "/usage/{wallet_address}",
        }
    }
}

pub enum ScorePath {
    GetTop,
    GetAccountPoint,
}

impl ScorePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GetTop => "/reward/top",
            Self::GetAccountPoint => "/reward",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            Self::GetTop => "/reward/top",
            Self::GetAccountPoint => "/reward",
        }
    }
}
