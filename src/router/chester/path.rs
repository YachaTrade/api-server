pub enum ChesterPath {
    Volume,
    Round,
    Rewards,
}

impl ChesterPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChesterPath::Volume => "/chester/volume/:account_id",
            ChesterPath::Round => "/chester/round",
            ChesterPath::Rewards => "/chester/rewards",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            ChesterPath::Volume => "/chester/volume/{account_id}",
            ChesterPath::Round => "/chester/round",
            ChesterPath::Rewards => "/chester/rewards",
        }
    }
}
