pub enum OrderPath {
    Creationtime,
    MarketCap,
    LatestTrade,
    ReplyCount,
    LatestReply,
}

impl OrderPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderPath::Creationtime => "/order/creation_time",
            OrderPath::MarketCap => "/order/market_cap",
            OrderPath::LatestTrade => "/order/latest_trade",
            OrderPath::ReplyCount => "/order/reply_count",
            OrderPath::LatestReply => "/order/latest_reply",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            OrderPath::Creationtime => "/order/creation_time",
            OrderPath::MarketCap => "/order/market_cap",
            OrderPath::LatestTrade => "/order/latest_trade",
            OrderPath::ReplyCount => "/order/reply_count",
            OrderPath::LatestReply => "/order/latest_reply",
        }
    }
}
