#[derive(Debug)]
pub enum HypePath {
    GetHype,
    GetEpoch,
    GetPoint,
    GetVoteHistory,
    GetPointHistory,
    GetRewardHistory,
    Vote,
}

impl HypePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            HypePath::GetHype => "/hype/token",
            HypePath::GetEpoch => "/hype/epoch",
            HypePath::GetPoint => "/hype/point",
            HypePath::GetVoteHistory => "/hype/vote_history",
            HypePath::GetPointHistory => "/hype/point_history",
            HypePath::GetRewardHistory => "/hype/reward_history",
            HypePath::Vote => "/hype/vote",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            HypePath::GetHype => "/hype/token",
            HypePath::GetEpoch => "/hype/epoch",
            HypePath::GetPoint => "/hype/point",
            HypePath::GetVoteHistory => "/hype/vote_history",
            HypePath::GetPointHistory => "/hype/point_history",
            HypePath::GetRewardHistory => "/hype/reward_history",
            HypePath::Vote => "/hype/vote",
        }
    }
}
