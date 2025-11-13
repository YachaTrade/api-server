use std::fmt;

pub enum RafflePath {
    GetStatus,
    GetPrizes,
}

impl RafflePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            RafflePath::GetStatus => "/raffle/status",
            RafflePath::GetPrizes => "/raffle/prizes",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            RafflePath::GetStatus => "/raffle/status",
            RafflePath::GetPrizes => "/raffle/prizes",
        }
    }
}


