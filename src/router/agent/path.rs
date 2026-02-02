#[derive(Debug, Clone, Copy)]
pub enum AgentPath {
    GetChart,
    GetSwapHistory,
    GetMarket,
    GetMetrics,
    GetToken,
    GetHoldings,
    UploadImage,
    UploadMetadata,
    GetTokensCreated,
    Salt,
}

impl AgentPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            AgentPath::GetChart => "/agent/chart/:token_id",
            AgentPath::GetSwapHistory => "/agent/swap-history/:token_id",
            AgentPath::GetMarket => "/agent/market/:token_id",
            AgentPath::GetMetrics => "/agent/metrics/:token_id",
            AgentPath::GetToken => "/agent/token/:token_id",
            AgentPath::GetHoldings => "/agent/holdings/:account_id",
            AgentPath::UploadImage => "/agent/token/image",
            AgentPath::UploadMetadata => "/agent/token/metadata",
            AgentPath::GetTokensCreated => "/agent/token/created/:account_id",
            AgentPath::Salt => "/agent/salt",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            AgentPath::GetChart => "/agent/chart/{token_id}",
            AgentPath::GetSwapHistory => "/agent/swap-history/{token_id}",
            AgentPath::GetMarket => "/agent/market/{token_id}",
            AgentPath::GetMetrics => "/agent/metrics/{token_id}",
            AgentPath::GetToken => "/agent/token/{token_id}",
            AgentPath::GetHoldings => "/agent/holdings/{account_id}",
            AgentPath::UploadImage => "/agent/token/image",
            AgentPath::UploadMetadata => "/agent/token/metadata",
            AgentPath::GetTokensCreated => "/agent/token/created/{account_id}",
            AgentPath::Salt => "/agent/salt",
        }
    }
}
