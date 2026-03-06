pub enum ChesterPath {
    Volume,
    Round,
    Rewards,
    SwapHistory,
    BoxRewards,
    RewardHistory,
}

impl ChesterPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChesterPath::Volume => "/chester/volume/:account_id",
            ChesterPath::Round => "/chester/round",
            ChesterPath::Rewards => "/chester/rewards",
            ChesterPath::SwapHistory => "/chester/history/:account_id",
            ChesterPath::BoxRewards => "/chester/box/rewards",
            ChesterPath::RewardHistory => "/chester/reward_history",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            ChesterPath::Volume => "/chester/volume/{account_id}",
            ChesterPath::Round => "/chester/round",
            ChesterPath::Rewards => "/chester/rewards",
            ChesterPath::SwapHistory => "/chester/history/{account_id}",
            ChesterPath::BoxRewards => "/chester/box/rewards",
            ChesterPath::RewardHistory => "/chester/reward_history",
        }
    }
}
