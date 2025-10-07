pub enum TradePath {
    GetSwapHistory,
    GetHolder,
    GetMarket,
    GetChart,
    GetManagementHistory,
    GetMetricsBatch,
}

impl TradePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            TradePath::GetSwapHistory => "/trade/swap-history/:token_id",
            TradePath::GetHolder => "/trade/holder/:token_id",
            TradePath::GetMarket => "/trade/market/:token_id",
            TradePath::GetChart => "/trade/chart/:token_id",
            TradePath::GetManagementHistory => "/trade/management-history/:token_id",
            TradePath::GetMetricsBatch => "/trade/metrics-batch/:token_id",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            TradePath::GetSwapHistory => "/trade/swap-history/{token_id}",
            TradePath::GetHolder => "/trade/holder/{token_id}",
            TradePath::GetMarket => "/trade/market/{token_id}",
            TradePath::GetChart => "/trade/chart/{token_id}",
            TradePath::GetManagementHistory => "/trade/management-history/{token_id}",
            TradePath::GetMetricsBatch => "/trade/metrics-batch/{token_id}",
        }
    }
}
