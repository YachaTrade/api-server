pub enum ChesterPath {
    Volume,
    Round,
    Rewards,
    SwapHistory,
}

impl ChesterPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChesterPath::Volume => "/chester/volume/:account_id",
            ChesterPath::Round => "/chester/round",
            ChesterPath::Rewards => "/chester/rewards",
            ChesterPath::SwapHistory => "/chester/history/:account_id",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            ChesterPath::Volume => "/chester/volume/{account_id}",
            ChesterPath::Round => "/chester/round",
            ChesterPath::Rewards => "/chester/rewards",
            ChesterPath::SwapHistory => "/chester/history/{account_id}",
        }
    }
}
