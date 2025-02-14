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

pub enum PointPath {
    GetTop,
    GetAccountPoint,
    CompleteMission,
    GetCompletedMissions,
}

impl PointPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GetTop => "/reward/top",
            Self::GetAccountPoint => "/reward/account",
            Self::CompleteMission => "/reward/complete",
            Self::GetCompletedMissions => "/reward/completed",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            Self::GetTop => "/reward/top",
            Self::GetAccountPoint => "/reward/account",
            Self::CompleteMission => "/reward/complete",
            Self::GetCompletedMissions => "/reward/completed",
        }
    }
}
