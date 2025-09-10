use std::env;
use std::sync::Arc;

use redis::{AsyncCommands, Client, aio::ConnectionManager, pipe};

use std::time::Instant;
use tracing::{debug, info};

use anyhow::Result;

use crate::{
    config::{
        GET_ACCOUNT_LOCKS_EXPIRATION, GET_ACCOUNT_WITHDRAWABLE_LOCK_EXPIRATION,
        GET_DEV_POSITIONS_EXPIRATION, GET_HOLDING_TOKEN_MANAGEMENT_EXPIRATION,
        GET_HYPE_TOKEN_RESPONSE_EXPIRATION, GET_TOKEN_MANAGEMENT_HISTORY_EXPIRATION,
        GET_TOKEN_METADATA_EXPIRATION, GET_TOKEN_RESPONSE_EXPIRATION, MESSAGE_EXPIRATION,
        NEW_CONTENT_EXPIRATION, NSFW_STATUS_EXPIRATION, ORDER_EXPIRATION, SEARCH_EXPIRATION, TOKEN_TRADE_EXPIRATION,
    },
    types::{
        common::pagination::PaginationParams,
        hype::{HypeEpochResponse, HypeTokenResponse, HypeVoteHistoryResponse, HypePointRecordResponse, HypePointResponse, HypeRewardHistoryResponse},
        management::{
            DevPositionsResponse, HoldingTokenManagementResponse, ManagementHistoryQuery,
            ManagementHistoryResponse, TokenLockResponse, WithdrawableLockResponse,
        },
        new_content::NewContentResponse,
        search::{SearchAccountResponse, SearchResponse, SearchTokenResponse},
        token::{
            TokenResponse,
            create_token::TokenCreatedResponse,
            metadata::TokenMetadataResponse,
            order::{OrderMessage, TokenOrderType},
        },
        trading::{
            chart::{BarResponse, GetBarsRequest},
            market::Market,
            position::{HoldTokenResponse, TokenHolderResponse},
            swap_history::{SwapQuery, TokenSwapResponse},
        },
    },
};

pub struct RedisDatabase {
    conn: Arc<ConnectionManager>,
}
impl RedisDatabase {
    pub async fn new() -> Self {
        let url = env::var("REDIS_URL")
            .unwrap_or_else(|_| panic!("REDIS_URL must be set in environment variables"));

        // Create Redis client - will handle rediss:// URLs automatically with native TLS
        let client = Client::open(url).expect("Failed to create Redis client");

        // Create connection manager for automatic reconnection
        let conn = ConnectionManager::new(client)
            .await
            .expect("Failed to create Redis connection manager");

        info!("Redis connection established with ElastiCache");

        RedisDatabase {
            conn: Arc::new(conn),
        }
    }

    //session

    //nonce -> address -> nonce
    pub async fn set_sign_message(&self, address: &str, message: &str) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

        let key = format!("session:{}:message", address);

        conn.pset_ex::<String, String, ()>(key, message.to_string(), *MESSAGE_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_sign_message(address: {}) completed in {:?}",
            address, elapsed
        );
        Ok(())
    }

    pub async fn get_sign_message(&self, address: &str) -> Result<String> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

        let key = format!("session:{}:message", address);
        let message: Option<String> = conn.get(key).await?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_sign_message(address: {}) completed in {:?}",
            address, elapsed
        );

        match message {
            Some(message) => Ok(message),
            None => Err(anyhow::anyhow!("Message not found")),
        }
    }

    pub async fn delete_sign_message(&self, address: &str) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

        let key = format!("session:{}:message", address);
        conn.del::<_, ()>(key).await?;

        let elapsed = start_time.elapsed();
        debug!(
            "delete_sign_message(address: {}) completed in {:?}",
            address, elapsed
        );
        Ok(())
    }

    //expire time = 10000
    pub async fn set_session(
        &self,
        session_id: &str,
        address: &str,
        expiration: u64,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

        let key = format!("session:{}:id", session_id);
        conn.pset_ex::<_, _, ()>(key, address, expiration).await?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_session(session_id: {}, address: {}, expiration: {}) completed in {:?}",
            session_id, address, expiration, elapsed
        );
        Ok(())
    }

    pub async fn get_address_by_session(&self, session_id: &str) -> Result<String> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("session:{}:id", session_id);
        let address = conn.get::<_, String>(key).await?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_address_by_session(session_id: {}) completed in {:?}",
            session_id, elapsed
        );
        Ok(address)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("session:{}:id", session_id);
        conn.del::<_, ()>(key).await?;

        let elapsed = start_time.elapsed();
        debug!(
            "delete_session(session_id: {}) completed in {:?}",
            session_id, elapsed
        );
        Ok(())
    }
}

//search response
impl RedisDatabase {
    pub async fn set_search_response(&self, query: &str, response: &SearchResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
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

        let elapsed = start_time.elapsed();
        debug!(
            "set_search_response(query: {}) completed in {:?}",
            query, elapsed
        );
        Ok(())
    }

    pub async fn get_search_response(
        &self,
        query: &str,
        pagination: PaginationParams,
    ) -> Result<Option<SearchResponse>> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
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

        let elapsed = start_time.elapsed();
        debug!(
            "get_search_response(query: {}, page: {}, limit: {}) completed in {:?}",
            query, pagination.page, pagination.limit, elapsed
        );
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
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

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

        let elapsed = start_time.elapsed();
        debug!(
            "set_order_response(order_type: {:?}, pagination: {:?}) completed in {:?}",
            order_type, pagination, elapsed
        );
        Ok(())
    }

    pub async fn get_order_response(
        &self,
        order_type: &TokenOrderType,
        pagination: Option<&PaginationParams>,
    ) -> Result<OrderMessage> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

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

        let elapsed = start_time.elapsed();
        debug!(
            "get_order_response(order_type: {:?}, pagination: {:?}) completed in {:?}",
            order_type, pagination, elapsed
        );
        Ok(response)
    }
}

impl RedisDatabase {
    pub async fn get_account_hold_token(
        &self,
        address: &str,
        pagination: &PaginationParams,
    ) -> Result<HoldTokenResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "hold_token:{}:page:{}:limit:{}",
            address, pagination.page, pagination.limit
        );
        let value: String = conn.get(key).await?;
        let response: HoldTokenResponse = serde_json::from_str(&value)?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_account_hold_token(address: {}, page: {}, limit: {}) completed in {:?}",
            address, pagination.page, pagination.limit, elapsed
        );
        Ok(response)
    }
    pub async fn set_account_hold_token(
        &self,
        address: &str,
        pagination: &PaginationParams,
        response: &HoldTokenResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "hold_token:{}:page:{}:limit:{}",
            address, pagination.page, pagination.limit
        );
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *TOKEN_TRADE_EXPIRATION)
            .await?;
        let elapsed = start_time.elapsed();
        debug!(
            "set_account_hold_token(address: {}, page: {}, limit: {}) completed in {:?}",
            address, pagination.page, pagination.limit, elapsed
        );
        Ok(())
    }

    pub async fn set_account_token_created(
        &self,
        address: &str,
        pagination: &PaginationParams,
        response: &TokenCreatedResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "create_token:{}:page:{}:limit:{}",
            address, pagination.page, pagination.limit
        );
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *TOKEN_TRADE_EXPIRATION)
            .await?;
        let elapsed = start_time.elapsed();
        debug!(
            "set_account_token_created(address: {}, page: {}, limit: {}) completed in {:?}",
            address, pagination.page, pagination.limit, elapsed
        );
        Ok(())
    }
    pub async fn get_account_token_created(
        &self,
        address: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenCreatedResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "create_token:{}:page:{}:limit:{}",
            address, pagination.page, pagination.limit
        );
        let response_json: String = conn.get(key).await?;
        let response: TokenCreatedResponse = serde_json::from_str(&response_json)?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_account_token_created(address: {}, page: {}, limit: {}) completed in {:?}",
            address, pagination.page, pagination.limit, elapsed
        );
        Ok(response)
    }
}

impl RedisDatabase {
    pub async fn set_token_response(&self, token_id: &str, response: &TokenResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("token:{}", token_id);

        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_TOKEN_RESPONSE_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_token_response(token_id: {}) completed in {:?}",
            token_id, elapsed
        );
        Ok(())
    }

    pub async fn get_token_response(&self, token_id: &str) -> Result<TokenResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("token:{}", token_id);

        let token_json: String = conn.get(key).await?;
        let response_json: TokenResponse = serde_json::from_str(&token_json)?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_token_response(token_id: {}) completed in {:?}",
            token_id, elapsed
        );
        Ok(response_json)
    }

    pub async fn set_token_metadata(
        &self,
        token_address: &str,
        response: &TokenMetadataResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("token_metadata:{}", token_address);
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *GET_TOKEN_METADATA_EXPIRATION)
            .await?;
        let elapsed = start_time.elapsed();
        debug!(
            "set_token_metadata(token_address: {}) completed in {:?}",
            token_address, elapsed
        );
        Ok(())
    }

    pub async fn get_token_metadata(&self, token_address: &str) -> Result<TokenMetadataResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("token_metadata:{}", token_address);
        let response_json: String = conn.get(key).await?;
        let response: TokenMetadataResponse = serde_json::from_str(&response_json)?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_token_metadata(token_address: {}) completed in {:?}",
            token_address, elapsed
        );
        Ok(response)
    }
}

// Hype cache methods
impl RedisDatabase {
    pub async fn set_hype_token_response(&self, response: &HypeTokenResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_token");
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!("set_hype_token_response() completed in {:?}", elapsed);
        Ok(())
    }

    pub async fn get_hype_token_response(&self) -> Result<HypeTokenResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_token");
        let response_json: String = conn.get(key).await?;
        let elapsed = start_time.elapsed();
        debug!("get_hype_token_response() completed in {:?}", elapsed);
        let response_json: HypeTokenResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn set_hype_epoch_response(&self, response: &HypeEpochResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "hype_epoch";
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(
            key.to_string(),
            json,
            *GET_HYPE_TOKEN_RESPONSE_EXPIRATION,
        )
        .await?;

        let elapsed = start_time.elapsed();
        debug!("set_hype_epoch_response() completed in {:?}", elapsed);
        Ok(())
    }

    pub async fn get_hype_epoch_response(&self) -> Result<HypeEpochResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "hype_epoch";
        let response_json: String = conn.get(key).await?;
        let elapsed = start_time.elapsed();
        debug!("get_hype_epoch_response() completed in {:?}", elapsed);
        let response: HypeEpochResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }

    pub async fn set_hype_vote_history_response(&self, account_id: &str, params: &PaginationParams, response: &HypeVoteHistoryResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_vote_history:{}:{}:{}", account_id, params.page, params.limit);
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!("set_hype_vote_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}", account_id, params.page, params.limit, elapsed);
        Ok(())
    }

    pub async fn get_hype_vote_history_response(&self, account_id: &str, params: &PaginationParams) -> Result<HypeVoteHistoryResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_vote_history:{}:{}:{}", account_id, params.page, params.limit);
        let response_json: String = conn.get(key).await?;
        let elapsed = start_time.elapsed();
        debug!("get_hype_vote_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}", account_id, params.page, params.limit, elapsed);
        let response: HypeVoteHistoryResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }

    pub async fn set_hype_point_history_response(&self, account_id: &str, params: &PaginationParams, response: &HypePointRecordResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_point_history:{}:{}:{}", account_id, params.page, params.limit);
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!("set_hype_point_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}", account_id, params.page, params.limit, elapsed);
        Ok(())
    }

    pub async fn get_hype_point_history_response(&self, account_id: &str, params: &PaginationParams) -> Result<HypePointRecordResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_point_history:{}:{}:{}", account_id, params.page, params.limit);
        let response_json: String = conn.get(key).await?;
        let elapsed = start_time.elapsed();
        debug!("get_hype_point_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}", account_id, params.page, params.limit, elapsed);
        let response: HypePointRecordResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }

    pub async fn set_hype_point_response(&self, account_id: &str, response: &HypePointResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_point:{}", account_id);
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!("set_hype_point_response(account_id: {}) completed in {:?}", account_id, elapsed);
        Ok(())
    }

    pub async fn get_hype_point_response(&self, account_id: &str) -> Result<HypePointResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_point:{}", account_id);
        let response_json: String = conn.get(key).await?;
        let elapsed = start_time.elapsed();
        debug!("get_hype_point_response(account_id: {}) completed in {:?}", account_id, elapsed);
        let response: HypePointResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }

    pub async fn set_hype_reward_history_response(&self, account_id: &str, params: &PaginationParams, response: &HypeRewardHistoryResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_reward_history:{}:{}:{}", account_id, params.page, params.limit);
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!("set_hype_reward_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}", account_id, params.page, params.limit, elapsed);
        Ok(())
    }

    pub async fn get_hype_reward_history_response(&self, account_id: &str, params: &PaginationParams) -> Result<HypeRewardHistoryResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_reward_history:{}:{}:{}", account_id, params.page, params.limit);
        let response_json: String = conn.get(key).await?;
        let elapsed = start_time.elapsed();
        debug!("get_hype_reward_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}", account_id, params.page, params.limit, elapsed);
        let response: HypeRewardHistoryResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }
}

// Market cache methods
impl RedisDatabase {}

//@@@@@@@@@@@Treasury@@@@@@@@@@@@@@@@@@@@@@@

impl RedisDatabase {
    pub async fn set_dev_positions(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        response: &DevPositionsResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "dev_positions:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_DEV_POSITIONS_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_dev_positions(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
        Ok(())
    }

    pub async fn get_dev_positions(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<DevPositionsResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "dev_positions:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let response_json: String = conn.get(key).await?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_dev_positions(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
        let response_json: DevPositionsResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn set_holding_token_management(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        response: &HoldingTokenManagementResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "holding_token_treasury:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_HOLDING_TOKEN_MANAGEMENT_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_holding_token_management(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
        Ok(())
    }

    pub async fn get_holding_token_management(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<HoldingTokenManagementResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "holding_token_management:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let response_json: String = conn.get(key).await?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_holding_token_management(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
        let response_json: HoldingTokenManagementResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn get_account_locks(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenLockResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "account_locks:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let response_json: String = conn.get(key).await?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_account_locks(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
        let response_json: TokenLockResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn set_account_locks(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        response: &TokenLockResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "account_locks:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_ACCOUNT_LOCKS_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_account_locks(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
        Ok(())
    }

    pub async fn get_account_withdrawable_lock(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<WithdrawableLockResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "account_withdrawable_lock:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let response_json: String = conn.get(key).await?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_account_withdrawable_lock(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
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
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "account_withdrawable_lock:{}:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_ACCOUNT_WITHDRAWABLE_LOCK_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_account_withdrawable_lock(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
        Ok(())
    }

    pub async fn get_token_management_history(
        &self,
        token_id: &str,
        query: &ManagementHistoryQuery,
    ) -> Result<ManagementHistoryResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("token:{}:management_history:query:{:?}", token_id, query);
        let response_json: String = conn.get(key).await?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_token_management_history(token_id: {}, query: {:?}) completed in {:?}",
            token_id, query, elapsed
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
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("token:{}:management_history:query:{:?}", token_id, query);
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *GET_TOKEN_MANAGEMENT_HISTORY_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_token_management_history(token_id: {}, query: {:?}) completed in {:?}",
            token_id, query, elapsed
        );
        Ok(())
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
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("token:{}:swap_history:query:{:?}", token_id, swap_query);
        let history_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, history_json, *TOKEN_TRADE_EXPIRATION)
            .await?;
        let elapsed = start_time.elapsed();
        debug!(
            "set_token_swap_history(token_id: {}, query: {:?}) completed in {:?}",
            token_id, swap_query, elapsed
        );
        Ok(())
    }

    pub async fn get_token_swap_history(
        &self,
        token_id: &str,
        swap_query: &SwapQuery,
    ) -> Result<TokenSwapResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("token:{}:swap_history:query:{:?}", token_id, swap_query);
        let history_json: String = conn.get(key).await?;
        let history: TokenSwapResponse = serde_json::from_str(&history_json)?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_token_swap_history(token_id: {}, query: {:?}) completed in {:?}",
            token_id, swap_query, elapsed
        );
        Ok(history)
    }

    pub async fn set_token_holder_response(
        &self,
        token_id: &str,
        response: &TokenHolderResponse,
        pagination: &PaginationParams,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "token:{}:holder:{}:{}",
            token_id, pagination.limit, pagination.page
        );
        let history_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, history_json, *TOKEN_TRADE_EXPIRATION)
            .await?;
        let elapsed = start_time.elapsed();
        debug!(
            "set_token_holder_response(token_id: {}, page: {}, limit: {}) completed in {:?}",
            token_id, pagination.page, pagination.limit, elapsed
        );
        Ok(())
    }
    pub async fn get_token_holder_response(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<TokenHolderResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "token:{}:holder:{}:{}",
            token_id, pagination.limit, pagination.page
        );
        let history_json: String = conn.get(key).await?;
        let history: TokenHolderResponse = serde_json::from_str(&history_json)?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_token_holder_response(token_id: {}, page: {}, limit: {}) completed in {:?}",
            token_id, pagination.page, pagination.limit, elapsed
        );
        Ok(history)
    }
    pub async fn get_prices(
        &self,
        token_id: &str,
        request: &GetBarsRequest,
    ) -> Result<Option<BarResponse>> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "token:{}:chart:resolution:{}:from:{}_to:{}",
            token_id, request.resolution, request.from, request.to
        );
        let data: Option<String> = conn.get(&key).await?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_prices(token_id: {}, resolution: {}, from: {}, to: {}) completed in {:?}",
            token_id, request.resolution, request.from, request.to, elapsed
        );

        match data {
            Some(json) => {
                let bar_data = serde_json::from_str::<BarResponse>(&json)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize BarResponse: {}", e))?;
                Ok(Some(bar_data))
            }
            None => Ok(None),
        }
    }

    pub async fn set_prices(
        &self,
        token_id: &str,
        request: &GetBarsRequest,
        bar_data: &BarResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "token:{}:chart:resolution:{}:from:{}_to:{}",
            token_id, request.resolution, request.from, request.to
        );
        let json = serde_json::to_string(bar_data)?;
        conn.pset_ex::<String, String, ()>(key, json, *TOKEN_TRADE_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_prices(token_id: {}, resolution: {}, from: {}, to: {}) completed in {:?}",
            token_id, request.resolution, request.from, request.to, elapsed
        );
        Ok(())
    }

    pub async fn set_market(&self, token_id: &str, response: &Market) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("market:{}", token_id);
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key, json, *TOKEN_TRADE_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_market(token_id: {}) completed in {:?}",
            token_id, elapsed
        );
        Ok(())
    }

    pub async fn get_market(&self, token_id: &str) -> Result<Market> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("market:{}", token_id);
        let response_json: String = conn.get(key).await?;
        let response: Market = serde_json::from_str(&response_json)?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_market(token_id: {}) completed in {:?}",
            token_id, elapsed
        );
        Ok(response)
    }
}

//New Content
impl RedisDatabase {
    pub async fn get_new_content(&self) -> Result<NewContentResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "new_content:latest";
        let response_json: String = conn.get(key).await?;
        let response: NewContentResponse = serde_json::from_str(&response_json)?;

        let elapsed = start_time.elapsed();
        debug!("get_new_content() completed in {:?}", elapsed);
        Ok(response)
    }

    pub async fn set_new_content(&self, response: &NewContentResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "new_content:latest";
        let json = serde_json::to_string(response)?;
        conn.pset_ex::<String, String, ()>(key.to_string(), json, *NEW_CONTENT_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!("set_new_content() completed in {:?}", elapsed);
        Ok(())
    }
}

// NSFW Caching
impl RedisDatabase {
    pub async fn get_nsfw_status(&self, image_url: &str) -> Result<bool> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("nsfw:{}", image_url);
        let is_nsfw: bool = conn.get(key).await?;

        let elapsed = start_time.elapsed();
        debug!("get_nsfw_status(url: {}) completed in {:?}", image_url, elapsed);
        Ok(is_nsfw)
    }

    pub async fn set_nsfw_status(&self, image_url: &str, is_nsfw: bool) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("nsfw:{}", image_url);
        
        // Cache using NSFW_STATUS_EXPIRATION (3 minutes = 180000 ms)
        conn.pset_ex::<String, bool, ()>(key, is_nsfw, *NSFW_STATUS_EXPIRATION)
            .await?;

        let elapsed = start_time.elapsed();
        debug!("set_nsfw_status(url: {}, is_nsfw: {}) completed in {:?}", image_url, is_nsfw, elapsed);
        Ok(())
    }

    pub async fn delete_nsfw_status(&self, image_url: &str) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("nsfw:{}", image_url);
        
        conn.del::<String, ()>(key).await?;

        let elapsed = start_time.elapsed();
        debug!("delete_nsfw_status(url: {}) completed in {:?}", image_url, elapsed);
        Ok(())
    }
}
