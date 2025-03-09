use std::env;

use deadpool_redis::{
    redis::{pipe, AsyncCommands},
    Config, PoolConfig, Runtime,
};

use tracing::{debug, info};

use anyhow::Result;

use crate::{
    config::{
        ACCOUNT_POINT_EXPIRATION, GET_TOKEN_RESPONSE_EXPIRATION, INVITED_CREATE_EXPIRATION,
        MISSION_EXPIRATION, NONCE_EXPIRATION, ORDER_EXPIRATION, PNL_EXPIRATION,
        POSITION_EXPIRATION, REFERRAL_CHILD_COUNT_EXPIRATION, REFERRAL_CODE_EXPIRATION,
        SEARCH_EXPIRATION, TOKEN_EXPIRATION, TOP_POINT_EXPIRATION,
    },
    types::{
        campaign::point::{
            AccountPointResponse, MissionCompleteResponse, MissionCompletedResponse,
            TopPointResponse,
        },
        common::pagination::PaginationParams,
        referral::{
            GetInvitedCreateResponse, GetReferralChildCountResponse, GetReferralCodeResponse,
        },
        search::{SearchAccountResponse, SearchResponse, SearchTokenResponse},
        token::{
            create_token::TokenCreatedResponse,
            order::{OrderMessage, TokenOrderType},
            TokenResponse,
        },
        trading::{
            chart::{ChartQuery, ChartResponse},
            pnl::PNLResponse,
            position::{PositionQuery, PositionResponse, TokenHolderResponse},
            swap_history::TokenSwapResponse,
        },
    },
};

pub struct RedisDatabase {
    pool: deadpool_redis::Pool, // 연결 풀 추가
}
impl RedisDatabase {
    pub async fn new_session_pool() -> Self {
        let url = env::var("SESSION_REDIS_URL").expect("SESSION_REDIS_URL must be set");
        let mut cfg = Config::from_url(url);

        cfg.pool = Some(PoolConfig {
            max_size: 100, // 최대 연결 수 증가
            timeouts: deadpool_redis::Timeouts {
                wait: Some(std::time::Duration::from_secs(5)),
                create: Some(std::time::Duration::from_secs(2)),
                recycle: Some(std::time::Duration::from_secs(1)),
            },
            queue_mode: deadpool::managed::QueueMode::Fifo,
        });

        let pool = cfg.create_pool(Some(Runtime::Tokio1)).unwrap();

        RedisDatabase { pool }
    }

    pub async fn new_trade_pool() -> Self {
        let url = env::var("TRADE_REDIS_URL").expect("TRADE_REDIS_URL must be set");
        let mut cfg = Config::from_url(url);

        cfg.pool = Some(PoolConfig {
            max_size: 100, // 최대 연결 수 증가
            timeouts: deadpool_redis::Timeouts {
                wait: Some(std::time::Duration::from_secs(5)),
                create: Some(std::time::Duration::from_secs(2)),
                recycle: Some(std::time::Duration::from_secs(1)),
            },
            queue_mode: deadpool::managed::QueueMode::Fifo,
        });

        let pool = cfg.create_pool(Some(Runtime::Tokio1)).unwrap();

        RedisDatabase { pool }
    }

    //session

    //nonce -> address -> nonce
    pub async fn set_nonce(&self, address: &str, nonce: &str) -> Result<()> {
        let mut conn = self.pool.get().await?;

        let key = format!("session:{}:nonce", address);

        conn.set_ex::<String, String, ()>(key, nonce.to_string(), *NONCE_EXPIRATION)
            .await?;

        Ok(())
    }

    pub async fn get_nonce(&self, address: &str) -> Result<String> {
        let mut conn = self.pool.get().await?;

        let key = format!("session:{}:nonce", address);
        let nonce: Option<String> = conn.get(key).await?;
        info!("Nonce for address {}: {:?}", address, nonce);
        match nonce {
            Some(nonce) => Ok(nonce),
            None => Err(anyhow::anyhow!("Nonce not found")),
        }
    }

    pub async fn del_nonce(&self, address: &str) -> Result<()> {
        let mut conn = self.pool.get().await?;

        let key = format!("session:{}:nonce", address);
        conn.del::<_, ()>(key).await?;
        debug!("Nonce deleted for address: {}", address);
        Ok(())
    }

    //expire time = 10000
    pub async fn set_session(
        &self,
        session_id: &str,
        address: &str,
        expiration: u64,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;

        let key = format!("session:{}:id", session_id);
        conn.set_ex::<_, _, ()>(key, address, expiration).await?;
        debug!(
            "Session set: {} -> {} (expires in {}s)",
            session_id, address, expiration
        );
        Ok(())
    }

    pub async fn get_address_by_session(&self, session_id: &str) -> Result<String> {
        let mut conn = self.pool.get().await?;
        let key = format!("session:{}:id", session_id);
        let address = conn.get::<_, String>(key).await?;
        debug!("Address for session {}: {:?}", session_id, address);
        Ok(address)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("session:{}:id", session_id);
        conn.del::<_, ()>(key).await?;
        debug!("Session deleted: {}", session_id);
        Ok(())
    }
}

//search response
impl RedisDatabase {
    pub async fn set_search_response(&self, query: &str, response: &SearchResponse) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let token_key = format!("search:{}:tokens", query);
        let account_key = format!("search:{}:accounts", query);

        // Serialize token and account responses separately
        let token_json = serde_json::to_string(&response.tokens)?;
        let account_json = serde_json::to_string(&response.accounts)?;

        // Use pipeline to set both values atomically
        let mut pipe = pipe();
        pipe.pset_ex(&token_key, token_json, *SEARCH_EXPIRATION)
            .pset_ex(&account_key, account_json, *SEARCH_EXPIRATION);

        let _: ((), ()) = pipe.query_async(&mut conn).await?;
        debug!("Search response set for query {}", query);
        Ok(())
    }

    pub async fn get_search_response(
        &self,
        query: &str,
        pagination: PaginationParams,
    ) -> Result<Option<SearchResponse>> {
        let mut conn = self.pool.get().await?;
        let token_key = format!("search:{}:tokens", query);
        let account_key = format!("search:{}:accounts", query);

        // Get both token and account responses
        let (token_json, account_json): (Option<String>, Option<String>) = pipe()
            .atomic()
            .get(&token_key)
            .get(&account_key)
            .query_async(&mut conn)
            .await?;

        // If either cache is missing, return None
        if token_json.is_none() || account_json.is_none() {
            return Ok(None);
        }

        let token_response: SearchTokenResponse = {
            let full_response: SearchTokenResponse = serde_json::from_str(&token_json.unwrap())?;
            let start_idx = (pagination.page - 1) * pagination.limit;

            // If no tokens in cache, return None
            if full_response.tokens.is_empty() {
                return Ok(None);
            }

            SearchTokenResponse {
                total_count: full_response.total_count,
                tokens: full_response
                    .tokens
                    .into_iter()
                    .skip(start_idx as usize)
                    .take(pagination.limit as usize)
                    .collect(),
            }
        };

        let account_response: SearchAccountResponse = {
            let full_response: SearchAccountResponse =
                serde_json::from_str(&account_json.unwrap())?;
            let start_idx = (pagination.page - 1) * pagination.limit;

            // If no accounts in cache, return None
            if full_response.accounts.is_empty() {
                return Ok(None);
            }

            SearchAccountResponse {
                total_count: full_response.total_count,
                accounts: full_response
                    .accounts
                    .into_iter()
                    .skip(start_idx as usize)
                    .take(pagination.limit as usize)
                    .collect(),
            }
        };

        debug!("Search response retrieved for query {}", query);
        Ok(Some(SearchResponse {
            tokens: token_response,
            accounts: account_response,
        }))
    }
}

//Order response
impl RedisDatabase {
    pub async fn set_order_response(
        &self,
        order_type: &TokenOrderType,
        response: &OrderMessage,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("order:{}:response", order_type.as_str());
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *ORDER_EXPIRATION)
            .await?;
        debug!("Order response set for order type {:?}", order_type);
        Ok(())
    }

    pub async fn get_order_response(&self, order_type: &TokenOrderType) -> Result<OrderMessage> {
        let mut conn = self.pool.get().await?;
        let key = format!("order:{}:response", order_type.as_str());
        let response_json: String = conn.get(key).await?;
        let response: OrderMessage = serde_json::from_str(&response_json)?;
        debug!("Order response retrieved for order type {:?}", order_type);
        Ok(response)
    }
}

//Token 관련
impl RedisDatabase {
    pub async fn set_token_swap_history(
        &self,
        token_id: &str,
        response: &TokenSwapResponse,
        pagination: &PaginationParams,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "token:{}:swap_history:{}:{}",
            token_id, pagination.limit, pagination.page
        );
        let history_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, history_json, *TOKEN_EXPIRATION)
            .await?;
        debug!("Token swap history set for token {:?}", token_id);
        Ok(())
    }

    pub async fn get_token_swap_history(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenSwapResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "token:{}:swap_history:{}:{}",
            token_id, pagination.limit, pagination.page
        );
        let history_json: String = conn.get(key).await?;
        let history: TokenSwapResponse = serde_json::from_str(&history_json)?;
        debug!("Token swap history retrieved for token {:?}", token_id);
        Ok(history)
    }

    pub async fn set_token_holder_response(
        &self,
        token_id: &str,
        response: &TokenHolderResponse,
        pagination: &PaginationParams,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "token:{}:holder:{}:{}",
            token_id, pagination.limit, pagination.page
        );
        let history_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, history_json, *TOKEN_EXPIRATION)
            .await?;
        debug!("Token holder set for token {:?}", token_id);
        Ok(())
    }
    pub async fn get_token_holder_response(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenHolderResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "token:{}:holder:{}:{}",
            token_id, pagination.limit, pagination.page
        );
        let history_json: String = conn.get(key).await?;
        let history: TokenHolderResponse = serde_json::from_str(&history_json)?;
        debug!("Token holder retrieved for token {:?}", token_id);
        Ok(history)
    }

    pub async fn set_chart_response(
        &self,
        token_id: &str,
        query: &ChartQuery,
        response: &ChartResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "token:{}:chart:interval:{}:base_timestamp:{}",
            token_id, query.interval, query.base_timestamp
        );
        let history_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, history_json, *TOKEN_EXPIRATION)
            .await?;
        debug!("Token chart set for token {:?}", token_id);
        Ok(())
    }
    pub async fn get_chart_response(
        &self,
        token_id: &str,
        query: &ChartQuery,
    ) -> Result<ChartResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "token:{}:chart:interval:{}:base_timestamp:{}",
            token_id, query.interval, query.base_timestamp
        );
        let history_json: String = conn.get(key).await?;
        let history: ChartResponse = serde_json::from_str(&history_json)?;
        debug!("Token chart retrieved for token {:?}", token_id);
        Ok(history)
    }
}

impl RedisDatabase {
    pub async fn set_top_point_response(
        &self,
        pagination: &PaginationParams,
        response: &TopPointResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("top_point:{}", pagination.limit);
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *TOP_POINT_EXPIRATION)
            .await?;

        Ok(())
    }
    pub async fn get_top_point_response(
        &self,
        pagination: &PaginationParams,
    ) -> Result<TopPointResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("top_point:{}", pagination.limit);
        let response_json: String = conn.get(key).await?;
        let response: TopPointResponse = serde_json::from_str(&response_json)?;
        debug!("Top point retrieved");
        Ok(response)
    }

    pub async fn set_point_by_account_id(
        &self,
        address: &str,
        response: &AccountPointResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("point:{}", address);
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *ACCOUNT_POINT_EXPIRATION)
            .await?;
        debug!("Point set for address {:?}", address);
        Ok(())
    }

    pub async fn get_point_by_account_id(&self, address: &str) -> Result<AccountPointResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("point:{}", address);
        let response_json: String = conn.get(key).await?;
        let response: AccountPointResponse = serde_json::from_str(&response_json)?;
        debug!("Point retrieved for address {:?}", address);
        Ok(response)
    }

    pub async fn set_complete_mission(
        &self,
        address: &str,
        response: &MissionCompleteResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("mission:{}", address);
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *MISSION_EXPIRATION)
            .await?;
        debug!("Point set for address {:?}", address);
        Ok(())
    }
    pub async fn get_complete_mission(&self, address: &str) -> Result<MissionCompleteResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("mission:{}", address);
        let response_json: String = conn.get(key).await?;
        let response: MissionCompleteResponse = serde_json::from_str(&response_json)?;
        debug!("Point retrieved for address {:?}", address);
        Ok(response)
    }

    pub async fn set_completed_missions(
        &self,
        address: &str,
        response: &MissionCompletedResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("completed_missions:{}", address);
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *MISSION_EXPIRATION)
            .await?;
        debug!("Point set for address {:?}", address);
        Ok(())
    }
    pub async fn get_completed_missions(&self, address: &str) -> Result<MissionCompletedResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("completed_missions:{}", address);
        let response_json: String = conn.get(key).await?;
        let response: MissionCompletedResponse = serde_json::from_str(&response_json)?;
        debug!("Point retrieved for address {:?}", address);
        Ok(response)
    }
}

impl RedisDatabase {
    pub async fn get_referral_code(&self, address: &str) -> Result<GetReferralCodeResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("referral_code:{}", address);
        let value: String = conn.get(key).await?;
        let response: GetReferralCodeResponse = serde_json::from_str(&value)?;
        Ok(response)
    }

    pub async fn set_referral_code(
        &self,
        address: &str,
        response: &GetReferralCodeResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("referral_code:{}", address);
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *REFERRAL_CODE_EXPIRATION)
            .await?;
        debug!("Point set for address {:?}", address);
        Ok(())
    }

    pub async fn get_referral_child_count(
        &self,
        address: &str,
    ) -> Result<GetReferralChildCountResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("referral_child_count:{}", address);
        let value: String = conn.get(key).await?;
        let response: GetReferralChildCountResponse = serde_json::from_str(&value)?;
        Ok(response)
    }
    pub async fn set_referral_child_count(
        &self,
        address: &str,
        response: &GetReferralChildCountResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("referral_child_count:{}", address);
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *REFERRAL_CHILD_COUNT_EXPIRATION)
            .await?;
        debug!("Point set for address {:?}", address);
        Ok(())
    }

    pub async fn get_invited_create(&self, address: &str) -> Result<GetInvitedCreateResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("invited_create:{}", address);
        let value: String = conn.get(key).await?;
        let response: GetInvitedCreateResponse = serde_json::from_str(&value)?;
        Ok(response)
    }

    pub async fn set_invited_create(
        &self,
        address: &str,
        response: &GetInvitedCreateResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("invited_create:{}", address);
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *INVITED_CREATE_EXPIRATION)
            .await?;

        Ok(())
    }
}

impl RedisDatabase {
    pub async fn get_account_pnl(&self, address: &str) -> Result<PNLResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("pnl:{}", address);
        let value: String = conn.get(key).await?;
        let response: PNLResponse = serde_json::from_str(&value)?;
        Ok(response)
    }

    pub async fn set_account_pnl(&self, address: &str, response: &PNLResponse) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("pnl:{}", address);
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *PNL_EXPIRATION)
            .await?;
        debug!("Point set for address {:?}", address);
        Ok(())
    }

    pub async fn get_account_position(
        &self,
        address: &str,
        position_query: &PositionQuery,
    ) -> Result<PositionResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "position:{}:type:{:?}:page:{}:limit:{}",
            address, position_query.position_type, position_query.page, position_query.limit
        );
        let value: String = conn.get(key).await?;
        let response: PositionResponse = serde_json::from_str(&value)?;
        Ok(response)
    }
    pub async fn set_account_position(
        &self,
        address: &str,
        position_query: &PositionQuery,
        response: &PositionResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "position:{}:type:{:?}:page:{}:limit:{}",
            address, position_query.position_type, position_query.page, position_query.limit
        );
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *POSITION_EXPIRATION)
            .await?;
        debug!("Position set for address {:?}", address);
        Ok(())
    }

    pub async fn set_account_token_created(
        &self,
        address: &str,
        pagination: &PaginationParams,
        response: &TokenCreatedResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "create_token:{}:page:{}:limit:{}",
            address, pagination.page, pagination.limit
        );
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *TOKEN_EXPIRATION)
            .await?;
        debug!("Token set for address {:?}", address);
        Ok(())
    }
    pub async fn get_account_token_created(
        &self,
        address: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenCreatedResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "create_token:{}:page:{}:limit:{}",
            address, pagination.page, pagination.limit
        );
        let response_json: String = conn.get(key).await?;
        let response: TokenCreatedResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }
}

impl RedisDatabase {
    pub async fn set_token_response(&self, token_id: &str, response: &TokenResponse) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("token:{}", token_id);

        let json = serde_json::to_string(response)?;
        conn.set_ex::<String, String, ()>(key, json, *GET_TOKEN_RESPONSE_EXPIRATION)
            .await?;

        Ok(())
    }

    pub async fn get_token_response(&self, token_id: &str) -> Result<TokenResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("token:{}", token_id);

        let token_json: String = conn.get(key).await?;
        let response_json: TokenResponse = serde_json::from_str(&token_json)?;
        Ok(response_json)
    }
}
