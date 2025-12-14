#[derive(Debug, Clone, Copy)]
pub enum TrendPath {
    GetTrend,
}

impl TrendPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            TrendPath::GetTrend => "/trend",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            TrendPath::GetTrend => "/trend",
        }
    }
}
