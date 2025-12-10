#[derive(Debug, Clone, Copy)]
pub enum LeaderboardPath {
    GetHypePointLeaderboard,
}

impl LeaderboardPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            LeaderboardPath::GetHypePointLeaderboard => "/leaderboard/hype_point",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            LeaderboardPath::GetHypePointLeaderboard => "/leaderboard/hype_point",
        }
    }
}
