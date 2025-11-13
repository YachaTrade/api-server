pub enum RafflePath {
    GetEligible,
    GetPrizes,
}

impl RafflePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            RafflePath::GetEligible => "/raffle/eligible",
            RafflePath::GetPrizes => "/raffle/prizes",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            RafflePath::GetEligible => "/raffle/eligible",
            RafflePath::GetPrizes => "/raffle/prizes",
        }
    }
}


