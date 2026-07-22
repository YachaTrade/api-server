#[derive(Debug, Clone, Copy)]
pub enum LeaderboardPath {
    GetPnlLeaderboard,
}

impl LeaderboardPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            LeaderboardPath::GetPnlLeaderboard => "/leaderboard/pnl",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            LeaderboardPath::GetPnlLeaderboard => "/leaderboard/pnl",
        }
    }
}
