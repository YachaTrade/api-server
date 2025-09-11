#[derive(Debug)]
pub enum HypePath {
    GetHype,
    GetEpoch,
    GetPoint,
    GetVoteHistory,
    GetPointHistory,
    GetRewardHistory,
    Vote,
    GetCommunityTreasury,
    GetTotalSpendPoint,
    GetRewardAddHistory,
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
            HypePath::GetCommunityTreasury => "/hype/community_treasury",
            HypePath::GetTotalSpendPoint => "/hype/total_spend_point",
            HypePath::GetRewardAddHistory => "/hype/reward_add_history",
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
            HypePath::GetCommunityTreasury => "/hype/community_treasury",
            HypePath::GetTotalSpendPoint => "/hype/total_spend_point",
            HypePath::GetRewardAddHistory => "/hype/reward_add_history",
            HypePath::Vote => "/hype/vote",
        }
    }
}
