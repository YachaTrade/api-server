pub enum OrderPath {
    Creationtime,
    MarketCap,
    LatestTrade,
    Hackathon,
}

impl OrderPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderPath::Creationtime => "/order/creation_time",
            OrderPath::MarketCap => "/order/market_cap",
            OrderPath::LatestTrade => "/order/latest_trade",
            OrderPath::Hackathon => "/order/hackathon",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            OrderPath::Creationtime => "/order/creation_time",
            OrderPath::MarketCap => "/order/market_cap",
            OrderPath::LatestTrade => "/order/latest_trade",
            OrderPath::Hackathon => "/order/hackathon",
        }
    }
}
