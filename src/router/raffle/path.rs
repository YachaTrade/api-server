pub enum RafflePath {
    GetEligible,
    Check,
}

impl RafflePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            RafflePath::GetEligible => "/raffle/eligible",
            RafflePath::Check => "/raffle/check",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            RafflePath::GetEligible => "/raffle/eligible",
            RafflePath::Check => "/raffle/check",
        }
    }
}


