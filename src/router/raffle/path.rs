pub enum RafflePath {
    GetEligible,
    Check,
    Round,
}

impl RafflePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            RafflePath::GetEligible => "/raffle/eligible",
            RafflePath::Check => "/raffle/check",
            RafflePath::Round => "/raffle/round",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            RafflePath::GetEligible => "/raffle/eligible",
            RafflePath::Check => "/raffle/check",
            RafflePath::Round => "/raffle/round",
        }
    }
}
