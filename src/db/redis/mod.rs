use std::env;

use deadpool_redis::{
    redis::{pipe, AsyncCommands},
    Config, PoolConfig, Runtime,
};

use tracing::{debug, info};

use anyhow::Result;

use crate::{
    config::{
        GET_ACCOUNT_LOCKS_EXPIRATION, GET_ACCOUNT_WITHDRAWABLE_LOCK_EXPIRATION,
        GET_DEV_POSITIONS_EXPIRATION, GET_HOLDING_TOKEN_MANAGEMENT_EXPIRATION,
        GET_HYPE_TOKEN_RESPONSE_EXPIRATION, GET_TOKEN_MANAGEMENT_HISTORY_EXPIRATION,
        GET_TOKEN_METADATA_EXPIRATION, GET_TOKEN_RESPONSE_EXPIRATION, HOLD_TOKEN_EXPIRATION,
        MESSAGE_EXPIRATION, ORDER_EXPIRATION, SEARCH_EXPIRATION, TOKEN_EXPIRATION,
    },
    types::{
        common::pagination::PaginationParams,
        management::{
            DevPositionsResponse, HoldingTokenManagementResponse, ManagementHistoryQuery,
            ManagementHistoryResponse, TokenLockResponse, WithdrawableLockResponse,
        },
        search::{SearchAccountResponse, SearchResponse, SearchTokenResponse},
        token::{
            create_token::TokenCreatedResponse,
            hype::HypeTokenResponse,
            metadata::TokenMetadataResponse,
            order::{OrderMessage, TokenOrderType},
            TokenResponse,
        },
        trading::{
            chart::{ChartQuery, ChartResponse},
            position::{HoldTokenResponse, TokenHolderResponse},
            swap_history::{SwapQuery, TokenSwapResponse},
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
    pub async fn set_sign_message(&self, address: &str, message: &str) -> Result<()> {
        let mut conn = self.pool.get().await?;

        let key = format!("session:{}:message", address);

        conn.set_ex::<String, String, ()>(key, message.to_string(), *MESSAGE_EXPIRATION)
            .await?;

        Ok(())
    }

    pub async fn get_sign_message(&self, address: &str) -> Result<String> {
        let mut conn = self.pool.get().await?;

        let key = format!("session:{}:message", address);
        let message: Option<String> = conn.get(key).await?;
        info!("Message for address {}: {:?}", address, message);
        match message {
            Some(message) => Ok(message),
            None => Err(anyhow::anyhow!("Message not found")),
        }
    }

    pub async fn delete_sign_message(&self, address: &str) -> Result<()> {
        let mut conn = self.pool.get().await?;

        let key = format!("session:{}:message", address);
        conn.del::<_, ()>(key).await?;
        debug!("Message deleted for address: {}", address);
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
        pagination: Option<&PaginationParams>,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;

        // 페이지네이션 파라미터가 있는 경우 키에 포함
        let key = match pagination {
            Some(params) => format!(
                "order:{}:response:page:{}_limit:{}",
                order_type.as_str(),
                params.page,
                params.limit
            ),
            None => format!("order:{}:response", order_type.as_str()),
        };

        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *ORDER_EXPIRATION)
            .await?;

        debug!(
            "Order response set for order type {:?} with pagination {:?}",
            order_type, pagination
        );
        Ok(())
    }

    pub async fn get_order_response(
        &self,
        order_type: &TokenOrderType,
        pagination: Option<&PaginationParams>,
    ) -> Result<OrderMessage> {
        let mut conn = self.pool.get().await?;

        // 페이지네이션 파라미터가 있는 경우 키에 포함
        let key = match pagination {
            Some(params) => format!(
                "order:{}:response:page:{}_limit:{}",
                order_type.as_str(),
                params.page,
                params.limit
            ),
            None => format!("order:{}:response", order_type.as_str()),
        };

        let response_json: String = conn.get(key).await?;
        let response: OrderMessage = serde_json::from_str(&response_json)?;

        debug!(
            "Order response retrieved for order type {:?} with pagination {:?}",
            order_type, pagination
        );
        Ok(response)
    }
}

//Token Page
impl RedisDatabase {
    pub async fn set_token_swap_history(
        &self,
        token_id: &str,
        response: &TokenSwapResponse,
        swap_query: &SwapQuery,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("token:{}:swap_history:query:{:?}", token_id, swap_query);
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
        swap_query: &SwapQuery,
    ) -> Result<TokenSwapResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("token:{}:swap_history:query:{:?}", token_id, swap_query);
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
    pub async fn get_account_hold_token(
        &self,
        address: &str,
        pagination: &PaginationParams,
    ) -> Result<HoldTokenResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "hold_token:{}:page:{}:limit:{}",
            address, pagination.page, pagination.limit
        );
        let value: String = conn.get(key).await?;
        let response: HoldTokenResponse = serde_json::from_str(&value)?;
        Ok(response)
    }
    pub async fn set_account_hold_token(
        &self,
        address: &str,
        pagination: &PaginationParams,
        response: &HoldTokenResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!(
            "hold_token:{}:page:{}:limit:{}",
            address, pagination.page, pagination.limit
        );
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *HOLD_TOKEN_EXPIRATION)
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
        conn.pset_ex::<String, String, ()>(key, json, *GET_TOKEN_RESPONSE_EXPIRATION)
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

    pub async fn set_token_metadata(
        &self,
        token_address: &str,
        response: &TokenMetadataResponse,
    ) -> Result<()> {
        let mut conn = self.pool.get().await?;
        let key = format!("token_metadata:{}", token_address);
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *GET_TOKEN_METADATA_EXPIRATION)
            .await?;
        debug!("Token metadata set for address {:?}", token_address);
        Ok(())
    }

    pub async fn get_token_metadata(&self, token_address: &str) -> Result<TokenMetadataResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("token_metadata:{}", token_address);
        let response_json: String = conn.get(key).await?;
        let response: TokenMetadataResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }
}

impl RedisDatabase {
    pub async fn set_hype_token_response(
        &self,
        pagination: &PaginationParams,
        response: &HypeTokenResponse,
    ) -> Result<()> {
        info!(
            "Set Hype Token: pagination: {:?}, response: {:?}",
            pagination, response
        );
        let mut conn = self.pool.get().await?;
        let key = format!(
            "hype_token:page:{}:limit:{}",
            pagination.page, pagination.limit
        );
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
            .await?;
        Ok(())
    }

    pub async fn get_hype_token_response(
        &self,
        pagination: &PaginationParams,
    ) -> Result<HypeTokenResponse> {
        info!("Get Hype Token: pagination: {:?}", pagination);
        let mut conn = self.pool.get().await?;
        let key = format!(
            "hype_token:page:{}:limit:{}",
            pagination.page, pagination.limit
        );
        let response_json: String = conn.get(key).await?;
        info!("Get Hype Token: response: {:?}", response_json);
        let response_json: HypeTokenResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }
}

//@@@@@@@@@@@Treasury@@@@@@@@@@@@@@@@@@@@@@@

impl RedisDatabase {
    pub async fn set_dev_positions(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        response: &DevPositionsResponse,
    ) -> Result<()> {
        info!(
            "Set Dev Positions: pagination: {:?}, response: {:?}",
            pagination, response
        );
        let mut conn = self.pool.get().await?;
        let key = format!(
            "dev_positions:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_DEV_POSITIONS_EXPIRATION)
            .await?;
        Ok(())
    }

    pub async fn get_dev_positions(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<DevPositionsResponse> {
        info!("Get Dev Positions: pagination: {:?}", pagination);
        let mut conn = self.pool.get().await?;
        let key = format!(
            "dev_positions:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let response_json: String = conn.get(key).await?;
        info!("Get Dev Positions: response: {:?}", response_json);
        let response_json: DevPositionsResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn set_holding_token_management(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        response: &HoldingTokenManagementResponse,
    ) -> Result<()> {
        info!(
            "Set Holding Token Treasury: pagination: {:?}, response: {:?}",
            pagination, response
        );
        let mut conn = self.pool.get().await?;
        let key = format!(
            "holding_token_treasury:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_HOLDING_TOKEN_MANAGEMENT_EXPIRATION)
            .await?;
        Ok(())
    }

    pub async fn get_holding_token_management(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HoldingTokenManagementResponse> {
        info!(
            "Get Holding Token Token Management: pagination: {:?}",
            pagination
        );
        let mut conn = self.pool.get().await?;
        let key = format!(
            "holding_token_management:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let response_json: String = conn.get(key).await?;
        info!(
            "Get Holding Token Token Management: response: {:?}",
            response_json
        );
        let response_json: HoldingTokenManagementResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn get_account_locks(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenLockResponse> {
        info!("Get Account Locks: pagination: {:?}", pagination);
        let mut conn = self.pool.get().await?;
        let key = format!(
            "account_locks:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let response_json: String = conn.get(key).await?;
        info!("Get Account Locks: response: {:?}", response_json);
        let response_json: TokenLockResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn set_account_locks(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        response: &TokenLockResponse,
    ) -> Result<()> {
        info!(
            "Set Account Locks: pagination: {:?}, response: {:?}",
            pagination, response
        );
        let mut conn = self.pool.get().await?;
        let key = format!(
            "account_locks:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_ACCOUNT_LOCKS_EXPIRATION)
            .await?;
        Ok(())
    }

    pub async fn get_account_withdrawable_lock(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<WithdrawableLockResponse> {
        info!(
            "Get Account Withdrawable Lock: pagination: {:?}",
            pagination
        );
        let mut conn = self.pool.get().await?;
        let key = format!(
            "account_withdrawable_lock:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let response_json: String = conn.get(key).await?;
        info!(
            "Get Account Withdrawable Lock: response: {:?}",
            response_json
        );
        let response_json: WithdrawableLockResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn set_account_withdrawable_lock(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        response: &WithdrawableLockResponse,
    ) -> Result<()> {
        info!(
            "Set Account Withdrawable Lock: pagination: {:?}, response: {:?}",
            pagination, response
        );
        let mut conn = self.pool.get().await?;
        let key = format!(
            "account_withdrawable_lock:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_ACCOUNT_WITHDRAWABLE_LOCK_EXPIRATION)
            .await?;
        Ok(())
    }

    pub async fn get_token_management_history(
        &self,
        token_id: &str,
        query: &ManagementHistoryQuery,
    ) -> Result<ManagementHistoryResponse> {
        let mut conn = self.pool.get().await?;
        let key = format!("token:{}:management_history:query:{:?}", token_id, query);
        let response_json: String = conn.get(key).await?;
        info!(
            "Get Token Management History: response: {:?}",
            response_json
        );
        let response_json: ManagementHistoryResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn set_token_management_history(
        &self,
        token_id: &str,
        query: &ManagementHistoryQuery,
        response: &ManagementHistoryResponse,
    ) -> Result<()> {
        info!(
            "Set Token Management History: query: {:?}, response: {:?}",
            query, response
        );
        let mut conn = self.pool.get().await?;
        let key = format!("token:{}:management_history:query:{:?}", token_id, query);
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_TOKEN_MANAGEMENT_HISTORY_EXPIRATION)
            .await?;
        Ok(())
    }
}
