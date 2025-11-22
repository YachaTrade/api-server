#[derive(Debug)]
pub enum HypePath {
    GetHype,
    GetHypeLatest,
    GetEpoch,
    GetPoint,
    GetVoteHistory,
    Vote,
    GetCommunityTreasury,
    GetTotalHypePoint,
    GetRewardAddHistory,
}

impl HypePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            HypePath::GetHype => "/hype/token",
            HypePath::GetHypeLatest => "/hype/token/latest",
            HypePath::GetEpoch => "/hype/epoch",
            HypePath::GetPoint => "/hype/point",
            HypePath::GetVoteHistory => "/hype/vote_history",
            HypePath::GetCommunityTreasury => "/hype/community_treasury",
            HypePath::GetTotalHypePoint => "/hype/total_hype_point",
            HypePath::GetRewardAddHistory => "/hype/reward_add_history",
            HypePath::Vote => "/hype/vote",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            HypePath::GetHype => "/hype/token",
            HypePath::GetHypeLatest => "/hype/token/latest",
            HypePath::GetEpoch => "/hype/epoch",
            HypePath::GetPoint => "/hype/point",
            HypePath::GetVoteHistory => "/hype/vote_history",
            HypePath::GetCommunityTreasury => "/hype/community_treasury",
            HypePath::GetTotalHypePoint => "/hype/total_hype_point",
            HypePath::GetRewardAddHistory => "/hype/reward_add_history",
            HypePath::Vote => "/hype/vote",
        }
    }
}
