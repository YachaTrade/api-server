pub enum TradePath {
    GetSwapHistory,
    GetHolder,
    GetMarket,
    GetChart,
}

impl TradePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradePath::GetSwapHistory => "/trade/swap-history/:token_id",
            TradePath::GetHolder => "/trade/holder/:token_id",
            TradePath::GetMarket => "/trade/market/:token_id",
            TradePath::GetChart => "/trade/chart/:token_id",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            TradePath::GetSwapHistory => "/trade/swap-history/{token_id}",
            TradePath::GetHolder => "/trade/holder/{token_id}",
            TradePath::GetMarket => "/trade/market/{token_id}",
            TradePath::GetChart => "/trade/chart/{token_id}",
        }
    }
}
