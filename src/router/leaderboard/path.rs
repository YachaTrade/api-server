#[derive(Debug, Clone, Copy)]
pub enum LeaderboardPath {
    GetHypePointLeaderboard,
    GetPnlLeaderboard,
}

impl LeaderboardPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            LeaderboardPath::GetHypePointLeaderboard => "/leaderboard/hype_point",
            LeaderboardPath::GetPnlLeaderboard => "/leaderboard/pnl",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            LeaderboardPath::GetHypePointLeaderboard => "/leaderboard/hype_point",
            LeaderboardPath::GetPnlLeaderboard => "/leaderboard/pnl",
        }
    }
}
