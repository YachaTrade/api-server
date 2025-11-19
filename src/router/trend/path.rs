use std::fmt::Display;

#[derive(Debug, Clone, Copy)]
pub enum TrendPath {
    GetTrend,
    InsertTrend,
    DeleteTrend,
}

impl TrendPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrendPath::GetTrend => "/trend",
            TrendPath::InsertTrend => "/trend/insert",
            TrendPath::DeleteTrend => "/trend/delete",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            TrendPath::GetTrend => "/trend",
            TrendPath::InsertTrend => "/trend/insert",
            TrendPath::DeleteTrend => "/trend/delete",
        }
    }
}

impl Display for TrendPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
