use std::env;
use std::sync::Arc;

use redis::{AsyncCommands, Client, aio::ConnectionManager, pipe};

use std::time::Instant;
use tracing::{debug, info};

use anyhow::Result;

use crate::{
    config::{
        GECKO_METADATA_EXPIRATION, GET_COMMUNITY_TREASURY_EXPIRATION,
        GET_HYPE_TOKEN_RESPONSE_EXPIRATION, GET_REWARD_ADD_HISTORY_EXPIRATION,
        GET_TOKEN_METADATA_EXPIRATION, GET_TOKEN_RESPONSE_EXPIRATION,
        GET_TOTAL_HYPE_POINT_EXPIRATION, GET_TREND_TOKEN_RESPONSE_EXPIRATION,
        HYPE_LEADERBOARD_RESPONSE_EXPIRATION, MESSAGE_EXPIRATION, NEW_CONTENT_EXPIRATION,
        NSFW_STATUS_EXPIRATION, ORDER_EXPIRATION, PNL_LEADERBOARD_RESPONSE_EXPIRATION,
        SEARCH_EXPIRATION, TOKEN_CREATED_EXPIRATION, TOKEN_TRADE_EXPIRATION,
    },
    measure_redis,
    types::{
        common::{info::AccountInfo, pagination::PaginationParams},
        hype::{
            AmountResponse, HypeEpochResponse, HypePointResponse, HypeRewardAddHistoryResponse,
            HypeTokenResponse, HypeVoteHistoryResponse,
        },
        leaderboard::{HypePointLeaderboardResponse, PnlLeaderboardResponse},
        metadata::TerminalMetadataResponse,
        new_event::NewEventResponse,
        profile::PointHistoryResponse,
        profile::{CreatedTokensResponse, HoldTokenResponse, SwapHistoryResponse},
        search::{AccountSearchResponse, SearchResponse, TokenSearchResponse},
        token::{
            TokenResponse,
            metadata::TokenMetadataResponse,
            order::{OrderTokenResponse, TokenOrderType},
        },
        trading::{
            chart::{BarResponse, GetBarsRequest},
            market::MarketResponse,
            position::TokenHolderResponse,
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

    /// Flush all Redis data
    pub async fn flush_all(&self) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        redis::cmd("FLUSHALL")
            .query_async::<()>(&mut conn)
            .await?;
        info!("Redis FLUSHALL completed");
        Ok(())
    }

    //session

    //nonce -> address -> nonce
    pub async fn set_sign_message(&self, address: &str, message: &str) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

        let key = format!("session:{}:message", address);

        measure_redis!(
            "redis.set_sign_message",
            conn.pset_ex::<String, String, ()>(key, message.to_string(), *MESSAGE_EXPIRATION)
        )?;

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
        let message: Option<String> =
            measure_redis!("redis.get_sign_message", conn.get::<_, Option<String>>(key))?;

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

    /// Atomically get and delete sign message to prevent nonce reuse attacks
    pub async fn get_and_delete_sign_message(&self, address: &str) -> Result<String> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

        let key = format!("session:{}:message", address);

        // Use GETDEL for atomic get-and-delete operation
        let message: Option<String> = measure_redis!(
            "redis.get_and_delete_sign_message",
            redis::cmd("GETDEL").arg(&key).query_async(&mut conn)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_and_delete_sign_message(address: {}) completed in {:?}",
            address, elapsed
        );

        match message {
            Some(message) => Ok(message),
            None => Err(anyhow::anyhow!("Message not found or already used")),
        }
    }

    pub async fn delete_sign_message(&self, address: &str) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

        let key = format!("session:{}:message", address);
        measure_redis!("redis.delete_sign_message", conn.del::<_, ()>(key))?;

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
        measure_redis!(
            "redis.set_session",
            conn.pset_ex::<_, _, ()>(key, address, expiration)
        )?;

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
        let address: String =
            measure_redis!("redis.get_address_by_session", conn.get::<_, String>(key))?;

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
        measure_redis!("redis.delete_session", conn.del::<_, ()>(key))?;

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
        let token_json = serde_json::to_string(&response.token_result)?;
        let account_json = serde_json::to_string(&response.account_result)?;

        // Use pipeline to set both values atomically
        let mut pipe = pipe();
        pipe.pset_ex(&token_key, token_json, *SEARCH_EXPIRATION)
            .pset_ex(&account_key, account_json, *SEARCH_EXPIRATION);

        let _: ((), ()) = measure_redis!("redis.set_search_response", pipe.query_async(&mut conn))?;

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
        let (token_json, account_json): (Option<String>, Option<String>) = measure_redis!(
            "redis.get_search_response",
            pipe()
                .atomic()
                .get(&token_key)
                .get(&account_key)
                .query_async(&mut conn)
        )?;

        // If either cache is missing, return None
        if token_json.is_none() || account_json.is_none() {
            return Ok(None);
        }

        let token_response: TokenSearchResponse = {
            let full_response: TokenSearchResponse = serde_json::from_str(&token_json.unwrap())?;
            let start_idx = (pagination.page - 1) * pagination.limit;

            // If no tokens in cache, return None
            if full_response.tokens.is_empty() {
                return Ok(None);
            }

            TokenSearchResponse {
                total_count: full_response.total_count,
                tokens: full_response
                    .tokens
                    .into_iter()
                    .skip(start_idx as usize)
                    .take(pagination.limit as usize)
                    .collect(),
            }
        };

        let account_response: AccountSearchResponse = {
            let full_response: AccountSearchResponse =
                serde_json::from_str(&account_json.unwrap())?;
            let start_idx = (pagination.page - 1) * pagination.limit;

            // If no accounts in cache, return None
            if full_response.accounts.is_empty() {
                return Ok(None);
            }

            AccountSearchResponse {
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
            token_result: token_response,
            account_result: account_response,
        }))
    }
}

//Order response
impl RedisDatabase {
    pub async fn set_order_response(
        &self,
        order_type: &TokenOrderType,
        response: &OrderTokenResponse,
        pagination: Option<&PaginationParams>,
        is_nsfw: bool,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

        // 페이지네이션 파라미터가 있는 경우 키에 포함
        let key = match pagination {
            Some(params) => format!(
                "order:{}:response:page:{}_limit:{}_direction:{}_nsfw:{}",
                order_type.as_str(),
                params.page,
                params.limit,
                params.direction,
                is_nsfw
            ),
            None => format!("order:{}:response:nsfw:{}", order_type.as_str(), is_nsfw),
        };

        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        measure_redis!(
            "redis.set_order_response",
            conn.pset_ex::<_, _, ()>(key, response_json, *ORDER_EXPIRATION)
        )?;

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
        is_nsfw: bool,
    ) -> Result<OrderTokenResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

        // 페이지네이션 파라미터가 있는 경우 키에 포함
        let key = match pagination {
            Some(params) => format!(
                "order:{}:response:page:{}_limit:{}_direction:{}_nsfw:{}",
                order_type.as_str(),
                params.page,
                params.limit,
                params.direction,
                is_nsfw
            ),
            None => format!("order:{}:response:nsfw:{}", order_type.as_str(), is_nsfw),
        };

        let response_json: String =
            measure_redis!("redis.get_order_response", conn.get::<_, String>(key))?;
        let response: OrderTokenResponse = serde_json::from_str(&response_json)?;

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
        let value: String =
            measure_redis!("redis.get_account_hold_token", conn.get::<_, String>(key))?;
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
        measure_redis!(
            "redis.set_account_hold_token",
            conn.pset_ex::<_, _, ()>(key, response_json, *TOKEN_TRADE_EXPIRATION)
        )?;
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
        response: &CreatedTokensResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "create_token:{}:page:{}:limit:{}",
            address, pagination.page, pagination.limit
        );
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        measure_redis!(
            "redis.set_account_token_created",
            conn.pset_ex::<_, _, ()>(key, response_json, *TOKEN_CREATED_EXPIRATION)
        )?;
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
    ) -> Result<CreatedTokensResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "create_token:{}:page:{}:limit:{}",
            address, pagination.page, pagination.limit
        );
        let response_json: String = measure_redis!(
            "redis.get_account_token_created",
            conn.get::<_, String>(key)
        )?;
        let response: CreatedTokensResponse = serde_json::from_str(&response_json)?;

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
        measure_redis!(
            "redis.set_token_response",
            conn.pset_ex::<String, String, ()>(key, json, *GET_TOKEN_RESPONSE_EXPIRATION)
        )?;

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

        let token_json: String =
            measure_redis!("redis.get_token_response", conn.get::<_, String>(key))?;
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
        measure_redis!(
            "redis.set_token_metadata",
            conn.pset_ex::<_, _, ()>(key, response_json, *GET_TOKEN_METADATA_EXPIRATION)
        )?;
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
        let response_json: String =
            measure_redis!("redis.get_token_metadata", conn.get::<_, String>(key))?;
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
        let key = "hype_token".to_string();
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_hype_token_response",
            conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!("set_hype_token_response() completed in {:?}", elapsed);
        Ok(())
    }

    pub async fn get_hype_token_response(&self) -> Result<HypeTokenResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "hype_token".to_string();
        let response_json: String =
            measure_redis!("redis.get_hype_token_response", conn.get::<_, String>(key))?;
        let elapsed = start_time.elapsed();
        debug!("get_hype_token_response() completed in {:?}", elapsed);
        let response_json: HypeTokenResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn set_hype_token_latest_response(&self, response: &HypeTokenResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "hype_token_latest".to_string();
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_hype_token_latest_response",
            conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_hype_token_latest_response() completed in {:?}",
            elapsed
        );
        Ok(())
    }

    pub async fn get_hype_token_latest_response(&self) -> Result<HypeTokenResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "hype_token_latest".to_string();
        let response_json: String = measure_redis!(
            "redis.get_hype_token_latest_response",
            conn.get::<_, String>(key)
        )?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_hype_token_latest_response() completed in {:?}",
            elapsed
        );
        let response_json: HypeTokenResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn set_hype_token_epoch_response(
        &self,
        epoch: i64,
        response: &HypeTokenResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_token_epoch:{}", epoch);
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_hype_token_epoch_response",
            conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_hype_token_epoch_response(epoch: {}) completed in {:?}",
            epoch, elapsed
        );
        Ok(())
    }

    pub async fn get_hype_token_epoch_response(&self, epoch: i64) -> Result<HypeTokenResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_token_epoch:{}", epoch);
        let response_json: String = measure_redis!(
            "redis.get_hype_token_epoch_response",
            conn.get::<_, String>(key)
        )?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_hype_token_epoch_response(epoch: {}) completed in {:?}",
            epoch, elapsed
        );
        let response: HypeTokenResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }

    pub async fn delete_hype_token_cache(&self) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();

        let keys = vec!["hype_token", "hype_token_latest"];
        measure_redis!(
            "redis.delete_hype_token_cache",
            conn.del::<&[&str], ()>(&keys)
        )?;

        let elapsed = start_time.elapsed();
        debug!("delete_hype_token_cache() completed in {:?}", elapsed);
        Ok(())
    }

    pub async fn set_total_hype_point_response(&self, response: &AmountResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "total_hype_point".to_string();
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_total_hype_point_response",
            conn.pset_ex::<String, String, ()>(key, json, *GET_TOTAL_HYPE_POINT_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!("set_total_hype_point_response() completed in {:?}", elapsed);
        Ok(())
    }

    pub async fn get_total_hype_point_response(&self) -> Result<AmountResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "total_hype_point".to_string();
        let response_json: String = measure_redis!(
            "redis.get_total_hype_point_response",
            conn.get::<_, String>(key)
        )?;
        let elapsed = start_time.elapsed();
        debug!("get_total_hype_point_response() completed in {:?}", elapsed);
        let response_json: AmountResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn set_community_treasury_response(&self, response: &AmountResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "community_treasury".to_string();
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_community_treasury_response",
            conn.pset_ex::<String, String, ()>(key, json, *GET_COMMUNITY_TREASURY_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_community_treasury_response() completed in {:?}",
            elapsed
        );
        Ok(())
    }

    pub async fn get_community_treasury_response(&self) -> Result<AmountResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "community_treasury".to_string();
        let response_json: String = measure_redis!(
            "redis.get_community_treasury_response",
            conn.get::<_, String>(key)
        )?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_community_treasury_response() completed in {:?}",
            elapsed
        );
        let response_json: AmountResponse = serde_json::from_str(&response_json)?;
        Ok(response_json)
    }

    pub async fn set_hype_epoch_response(&self, response: &HypeEpochResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "hype_epoch";
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_hype_epoch_response",
            conn.pset_ex::<String, String, ()>(
                key.to_string(),
                json,
                *GET_HYPE_TOKEN_RESPONSE_EXPIRATION,
            )
        )?;

        let elapsed = start_time.elapsed();
        debug!("set_hype_epoch_response() completed in {:?}", elapsed);
        Ok(())
    }

    pub async fn get_hype_epoch_response(&self) -> Result<HypeEpochResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "hype_epoch";
        let response_json: String =
            measure_redis!("redis.get_hype_epoch_response", conn.get::<_, String>(key))?;
        let elapsed = start_time.elapsed();
        debug!("get_hype_epoch_response() completed in {:?}", elapsed);
        let response: HypeEpochResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }

    pub async fn set_hype_vote_history_response(
        &self,
        account_id: &str,
        params: &PaginationParams,
        response: &HypeVoteHistoryResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "hype_vote_history:{}:{}:{}",
            account_id, params.page, params.limit
        );
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_hype_vote_history_response",
            conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_hype_vote_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, params.page, params.limit, elapsed
        );
        Ok(())
    }

    pub async fn get_hype_vote_history_response(
        &self,
        account_id: &str,
        params: &PaginationParams,
    ) -> Result<HypeVoteHistoryResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "hype_vote_history:{}:{}:{}",
            account_id, params.page, params.limit
        );
        let response_json: String = measure_redis!(
            "redis.get_hype_vote_history_response",
            conn.get::<_, String>(key)
        )?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_hype_vote_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, params.page, params.limit, elapsed
        );
        let response: HypeVoteHistoryResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }

    pub async fn set_hype_point_history_response(
        &self,
        account_id: &str,
        params: &PaginationParams,
        response: &PointHistoryResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "hype_point_history:{}:{}:{}",
            account_id, params.page, params.limit
        );
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_hype_point_history_response",
            conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_hype_point_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, params.page, params.limit, elapsed
        );
        Ok(())
    }

    pub async fn get_hype_point_history_response(
        &self,
        account_id: &str,
        params: &PaginationParams,
    ) -> Result<PointHistoryResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "hype_point_history:{}:{}:{}",
            account_id, params.page, params.limit
        );
        let response_json: String = measure_redis!(
            "redis.get_hype_point_history_response",
            conn.get::<_, String>(key)
        )?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_hype_point_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, params.page, params.limit, elapsed
        );
        let response: PointHistoryResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }

    pub async fn set_hype_point_response(
        &self,
        account_id: &str,
        response: &HypePointResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_point:{}", account_id);
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_hype_point_response",
            conn.pset_ex::<String, String, ()>(key, json, *GET_HYPE_TOKEN_RESPONSE_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_hype_point_response(account_id: {}) completed in {:?}",
            account_id, elapsed
        );
        Ok(())
    }

    pub async fn get_hype_point_response(&self, account_id: &str) -> Result<HypePointResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("hype_point:{}", account_id);
        let response_json: String =
            measure_redis!("redis.get_hype_point_response", conn.get::<_, String>(key))?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_hype_point_response(account_id: {}) completed in {:?}",
            account_id, elapsed
        );
        let response: HypePointResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }

    pub async fn set_hype_reward_add_history_response(
        &self,
        account_id: &str,
        params: &PaginationParams,
        response: &HypeRewardAddHistoryResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "hype_reward_add_history:{}:{}:{}",
            account_id, params.page, params.limit
        );
        let serialized = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_hype_reward_add_history_response",
            conn.pset_ex::<String, String, ()>(key, serialized, *GET_REWARD_ADD_HISTORY_EXPIRATION)
        )?;
        let elapsed = start_time.elapsed();
        debug!(
            "set_hype_reward_add_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, params.page, params.limit, elapsed
        );
        Ok(())
    }

    pub async fn get_hype_reward_add_history_response(
        &self,
        account_id: &str,
        params: &PaginationParams,
    ) -> Result<HypeRewardAddHistoryResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "hype_reward_add_history:{}:{}:{}",
            account_id, params.page, params.limit
        );
        let response_json: String = measure_redis!(
            "redis.get_hype_reward_add_history_response",
            conn.get::<_, String>(key)
        )?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_hype_reward_add_history_response(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, params.page, params.limit, elapsed
        );
        let response: HypeRewardAddHistoryResponse = serde_json::from_str(&response_json)?;
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
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("token:{}:swap_history:query:{:?}", token_id, swap_query);
        let history_json = serde_json::to_string(response)?;
        //pset is miliseconds
        measure_redis!(
            "redis.set_token_swap_history",
            conn.pset_ex::<_, _, ()>(key, history_json, *TOKEN_TRADE_EXPIRATION)
        )?;
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
        let history_json: String =
            measure_redis!("redis.get_token_swap_history", conn.get::<_, String>(key))?;
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
        measure_redis!(
            "redis.set_token_holder_response",
            conn.pset_ex::<_, _, ()>(key, history_json, *TOKEN_TRADE_EXPIRATION)
        )?;
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
        let history_json: String = measure_redis!(
            "redis.get_token_holder_response",
            conn.get::<_, String>(key)
        )?;
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
        let data: Option<String> =
            measure_redis!("redis.get_prices", conn.get::<_, Option<String>>(&key))?;

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
        measure_redis!(
            "redis.set_prices",
            conn.pset_ex::<String, String, ()>(key, json, *TOKEN_TRADE_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_prices(token_id: {}, resolution: {}, from: {}, to: {}) completed in {:?}",
            token_id, request.resolution, request.from, request.to, elapsed
        );
        Ok(())
    }

    pub async fn set_market(&self, token_id: &str, response: &MarketResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("market:{}", token_id);
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_market",
            conn.pset_ex::<String, String, ()>(key, json, *TOKEN_TRADE_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_market(token_id: {}) completed in {:?}",
            token_id, elapsed
        );
        Ok(())
    }

    pub async fn get_market(&self, token_id: &str) -> Result<MarketResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("market:{}", token_id);
        let response_json: String = measure_redis!("redis.get_market", conn.get::<_, String>(key))?;
        let response: MarketResponse = serde_json::from_str(&response_json)?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_market(token_id: {}) completed in {:?}",
            token_id, elapsed
        );
        Ok(response)
    }
}

//New Event
impl RedisDatabase {
    pub async fn get_new_event(&self) -> Result<NewEventResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "new_event:latest";
        let response_json: String =
            measure_redis!("redis.get_new_event", conn.get::<_, String>(key))?;
        let response: NewEventResponse = serde_json::from_str(&response_json)?;

        let elapsed = start_time.elapsed();
        debug!("get_new_event() completed in {:?}", elapsed);
        Ok(response)
    }

    pub async fn set_new_event(&self, response: &NewEventResponse) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "new_event:latest";
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_new_event",
            conn.pset_ex::<String, String, ()>(key.to_string(), json, *NEW_CONTENT_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!("set_new_event() completed in {:?}", elapsed);
        Ok(())
    }
}

// NSFW Caching
impl RedisDatabase {
    pub async fn get_nsfw_status(&self, image_url: &str) -> Result<Option<bool>> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("nsfw:{}", image_url);
        let is_nsfw: Option<bool> =
            measure_redis!("redis.get_nsfw_status", conn.get::<_, Option<bool>>(key))?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_nsfw_status(url: {}) completed in {:?}",
            image_url, elapsed
        );
        Ok(is_nsfw)
    }

    pub async fn set_nsfw_status(&self, image_url: &str, is_nsfw: bool) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("nsfw:{}", image_url);

        // Cache using NSFW_STATUS_EXPIRATION (3 minutes = 180000 ms)
        measure_redis!(
            "redis.set_nsfw_status",
            conn.pset_ex::<String, bool, ()>(key, is_nsfw, *NSFW_STATUS_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_nsfw_status(url: {}, is_nsfw: {}) completed in {:?}",
            image_url, is_nsfw, elapsed
        );
        Ok(())
    }

    pub async fn set_account_swap_history(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
        response: &SwapHistoryResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "account:{}:swap_history:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let history_json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_account_swap_history",
            conn.pset_ex::<_, _, ()>(key, history_json, *TOKEN_TRADE_EXPIRATION)
        )?;
        let elapsed = start_time.elapsed();
        debug!(
            "set_account_swap_history(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
        Ok(())
    }

    pub async fn get_account_swap_history(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<SwapHistoryResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!(
            "account:{}:swap_history:page:{}:limit:{}",
            account_id, pagination.page, pagination.limit
        );
        let history_json: String =
            measure_redis!("redis.get_account_swap_history", conn.get::<_, String>(key))?;
        let history: SwapHistoryResponse = serde_json::from_str(&history_json)?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_account_swap_history(account_id: {}, page: {}, limit: {}) completed in {:?}",
            account_id, pagination.page, pagination.limit, elapsed
        );
        Ok(history)
    }

    pub async fn delete_nsfw_status(&self, image_url: &str) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("nsfw:{}", image_url);

        measure_redis!("redis.delete_nsfw_status", conn.del::<String, ()>(key))?;

        let elapsed = start_time.elapsed();
        debug!(
            "delete_nsfw_status(url: {}) completed in {:?}",
            image_url, elapsed
        );
        Ok(())
    }
}

// Gecko Metadata Caching
impl RedisDatabase {
    pub async fn set_terminal_metadata(
        &self,
        token_address: &str,
        response: &TerminalMetadataResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("terminal_metadata:{}", token_address);
        let response_json = serde_json::to_string(response)?;

        measure_redis!(
            "redis.set_terminal_metadata",
            conn.pset_ex::<_, _, ()>(key, response_json, *GECKO_METADATA_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_terminal_metadata(token_address: {}) completed in {:?}",
            token_address, elapsed
        );
        Ok(())
    }

    pub async fn get_terminal_metadata(
        &self,
        token_address: &str,
    ) -> Result<Option<TerminalMetadataResponse>> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("terminal_metadata:{}", token_address);

        let response_json: Option<String> = measure_redis!(
            "redis.get_terminal_metadata",
            conn.get::<_, Option<String>>(key)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_terminal_metadata(token_address: {}) completed in {:?}",
            token_address, elapsed
        );

        match response_json {
            Some(json) => {
                let response: TerminalMetadataResponse = serde_json::from_str(&json)?;
                Ok(Some(response))
            }
            None => Ok(None),
        }
    }

    // Trend Caching
    pub async fn set_trend_response(
        &self,
        response: &crate::types::trend::TrendResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "trend:all";
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_trend_response",
            conn.pset_ex::<String, String, ()>(
                key.to_string(),
                json,
                *GET_TREND_TOKEN_RESPONSE_EXPIRATION
            )
        )?;

        let elapsed = start_time.elapsed();
        debug!("set_trend_response() completed in {:?}", elapsed);
        Ok(())
    }

    pub async fn get_trend_response(&self) -> Result<crate::types::trend::TrendResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = "trend:all";
        let response_json: String =
            measure_redis!("redis.get_trend_response", conn.get::<_, String>(key))?;
        let elapsed = start_time.elapsed();
        debug!("get_trend_response() completed in {:?}", elapsed);
        let response: crate::types::trend::TrendResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }

    // Account Info Caching
    pub async fn set_account_info(
        &self,
        account_id: &str,
        account_info: &AccountInfo,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("account:info:{}", account_id);
        let account_json = serde_json::to_string(account_info)?;

        measure_redis!(
            "redis.set_account_info",
            conn.pset_ex::<_, _, ()>(key, account_json, *SEARCH_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_account_info(account_id: {}) completed in {:?}",
            account_id, elapsed
        );
        Ok(())
    }

    pub async fn get_account_info(&self, account_id: &str) -> Result<Option<AccountInfo>> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("account:info:{}", account_id);

        let account_json: Option<String> =
            measure_redis!("redis.get_account_info", conn.get::<_, Option<String>>(key))?;

        let elapsed = start_time.elapsed();
        debug!(
            "get_account_info(account_id: {}) completed in {:?}",
            account_id, elapsed
        );

        match account_json {
            Some(json) => {
                let account_info: AccountInfo = serde_json::from_str(&json)?;
                Ok(Some(account_info))
            }
            None => Ok(None),
        }
    }
}

// Leaderboard cache
impl RedisDatabase {
    pub async fn set_hype_point_leaderboard_response(
        &self,
        page: i64,
        limit: i64,
        response: &HypePointLeaderboardResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("leaderboard:hype_point:page:{}:limit:{}", page, limit);
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_hype_point_leaderboard_response",
            conn.pset_ex::<String, String, ()>(key, json, *HYPE_LEADERBOARD_RESPONSE_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_hype_point_leaderboard_response(page: {}, limit: {}) completed in {:?}",
            page, limit, elapsed
        );
        Ok(())
    }

    pub async fn get_hype_point_leaderboard_response(
        &self,
        page: i64,
        limit: i64,
    ) -> Result<HypePointLeaderboardResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("leaderboard:hype_point:page:{}:limit:{}", page, limit);
        let response_json: String = measure_redis!(
            "redis.get_hype_point_leaderboard_response",
            conn.get::<_, String>(key)
        )?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_hype_point_leaderboard_response(page: {}, limit: {}) completed in {:?}",
            page, limit, elapsed
        );
        let response: HypePointLeaderboardResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }

    pub async fn set_pnl_leaderboard_response(
        &self,
        page: i64,
        limit: i64,
        response: &PnlLeaderboardResponse,
    ) -> Result<()> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("leaderboard:pnl:page:{}:limit:{}", page, limit);
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_pnl_leaderboard_response",
            conn.pset_ex::<String, String, ()>(key, json, *PNL_LEADERBOARD_RESPONSE_EXPIRATION)
        )?;

        let elapsed = start_time.elapsed();
        debug!(
            "set_pnl_leaderboard_response(page: {}, limit: {}) completed in {:?}",
            page, limit, elapsed
        );
        Ok(())
    }

    pub async fn get_pnl_leaderboard_response(
        &self,
        page: i64,
        limit: i64,
    ) -> Result<PnlLeaderboardResponse> {
        let start_time = Instant::now();
        let mut conn = self.conn.as_ref().clone();
        let key = format!("leaderboard:pnl:page:{}:limit:{}", page, limit);
        let response_json: String = measure_redis!(
            "redis.get_pnl_leaderboard_response",
            conn.get::<_, String>(key)
        )?;
        let elapsed = start_time.elapsed();
        debug!(
            "get_pnl_leaderboard_response(page: {}, limit: {}) completed in {:?}",
            page, limit, elapsed
        );
        let response: PnlLeaderboardResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }
}

// Chester cache
const CHESTER_CACHE_EXPIRATION: u64 = 10_000; // 10 seconds in ms
const CHESTER_REWARDS_CACHE_EXPIRATION: u64 = 60_000; // 60 seconds in ms

impl RedisDatabase {
    pub async fn set_chester_round(
        &self,
        response: &crate::types::chester::ChesterInfoResponse,
    ) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let key = "chester:round";
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_chester_round",
            conn.pset_ex::<String, String, ()>(key.to_string(), json, CHESTER_CACHE_EXPIRATION)
        )?;
        Ok(())
    }

    pub async fn get_chester_round(
        &self,
    ) -> Result<Option<crate::types::chester::ChesterInfoResponse>> {
        let mut conn = self.conn.as_ref().clone();
        let key = "chester:round";
        let json: Option<String> =
            measure_redis!("redis.get_chester_round", conn.get::<_, Option<String>>(key))?;
        match json {
            Some(j) => Ok(Some(serde_json::from_str(&j)?)),
            None => Ok(None),
        }
    }

    pub async fn set_chester_volume(
        &self,
        account_id: &str,
        response: &crate::types::chester::ChesterVolumeResponse,
    ) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let key = format!("chester:volume:{}", account_id);
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_chester_volume",
            conn.pset_ex::<String, String, ()>(key, json, CHESTER_CACHE_EXPIRATION)
        )?;
        Ok(())
    }

    pub async fn get_chester_volume(
        &self,
        account_id: &str,
    ) -> Result<Option<crate::types::chester::ChesterVolumeResponse>> {
        let mut conn = self.conn.as_ref().clone();
        let key = format!("chester:volume:{}", account_id);
        let json: Option<String> =
            measure_redis!("redis.get_chester_volume", conn.get::<_, Option<String>>(key))?;
        match json {
            Some(j) => Ok(Some(serde_json::from_str(&j)?)),
            None => Ok(None),
        }
    }

    pub async fn set_chester_rewards(
        &self,
        response: &crate::types::chester::ChesterRewardsResponse,
    ) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let key = "chester:rewards";
        let json = serde_json::to_string(response)?;
        measure_redis!(
            "redis.set_chester_rewards",
            conn.pset_ex::<String, String, ()>(key.to_string(), json, CHESTER_REWARDS_CACHE_EXPIRATION)
        )?;
        Ok(())
    }

    pub async fn get_chester_rewards(
        &self,
    ) -> Result<Option<crate::types::chester::ChesterRewardsResponse>> {
        let mut conn = self.conn.as_ref().clone();
        let key = "chester:rewards";
        let json: Option<String> =
            measure_redis!("redis.get_chester_rewards", conn.get::<_, Option<String>>(key))?;
        match json {
            Some(j) => Ok(Some(serde_json::from_str(&j)?)),
            None => Ok(None),
        }
    }
}

// API Key Rate Limiting
impl RedisDatabase {
    /// Atomic INCR with EXPIRE (for rate limiting)
    /// Returns the current count after increment
    pub async fn incr_with_expire(&self, key: &str, ttl_secs: u64) -> Result<u64> {
        let mut conn = self.conn.as_ref().clone();

        // Use INCR command
        let count: u64 = redis::cmd("INCR")
            .arg(key)
            .query_async(&mut conn)
            .await?;

        // Set TTL only on first increment (when count == 1)
        if count == 1 {
            let _: () = redis::cmd("EXPIRE")
                .arg(key)
                .arg(ttl_secs)
                .query_async(&mut conn)
                .await?;
        }

        Ok(count)
    }

    /// Cache API key info (JSON string)
    pub async fn cache_api_key(&self, key_hash: &str, info: &str, ttl_secs: u64) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let redis_key = format!("api_key:{}", key_hash);

        let _: () = redis::cmd("SETEX")
            .arg(&redis_key)
            .arg(ttl_secs)
            .arg(info)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    /// Get cached API key info
    pub async fn get_cached_api_key(&self, key_hash: &str) -> Result<Option<String>> {
        let mut conn = self.conn.as_ref().clone();
        let redis_key = format!("api_key:{}", key_hash);

        let result: Option<String> = redis::cmd("GET")
            .arg(&redis_key)
            .query_async(&mut conn)
            .await?;

        Ok(result)
    }

    /// Delete cached API key (on revocation)
    pub async fn delete_cached_api_key(&self, key_hash: &str) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let redis_key = format!("api_key:{}", key_hash);

        let _: () = redis::cmd("DEL")
            .arg(&redis_key)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    /// Increment API key usage counter in Redis (fast, in-memory)
    /// Returns the current count after increment
    pub async fn incr_api_key_usage(&self, key_hash: &str) -> Result<i64> {
        let mut conn = self.conn.as_ref().clone();
        let redis_key = format!("apikey:usage:{}", key_hash);

        let count: i64 = redis::cmd("INCR")
            .arg(&redis_key)
            .query_async(&mut conn)
            .await?;

        Ok(count)
    }

    /// Get API key usage count from Redis
    pub async fn get_api_key_usage(&self, key_hash: &str) -> Result<i64> {
        let mut conn = self.conn.as_ref().clone();
        let redis_key = format!("apikey:usage:{}", key_hash);

        let count: Option<i64> = redis::cmd("GET")
            .arg(&redis_key)
            .query_async(&mut conn)
            .await?;

        Ok(count.unwrap_or(0))
    }

    /// Get and reset API key usage count (atomic GETSET with 0)
    /// Used for periodic DB sync
    pub async fn get_and_reset_api_key_usage(&self, key_hash: &str) -> Result<i64> {
        let mut conn = self.conn.as_ref().clone();
        let redis_key = format!("apikey:usage:{}", key_hash);

        // GETSET returns old value and sets new value atomically
        let count: Option<i64> = redis::cmd("GETSET")
            .arg(&redis_key)
            .arg(0i64)
            .query_async(&mut conn)
            .await?;

        Ok(count.unwrap_or(0))
    }

    /// Get all API key usage keys (for batch sync)
    /// Uses SCAN instead of KEYS to avoid blocking Redis
    pub async fn get_all_api_key_usage_keys(&self) -> Result<Vec<String>> {
        let mut conn = self.conn.as_ref().clone();
        let mut keys = Vec::new();
        let mut cursor: u64 = 0;

        loop {
            let (new_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg("apikey:usage:*")
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await?;

            keys.extend(batch);
            cursor = new_cursor;

            if cursor == 0 {
                break;
            }
        }

        Ok(keys)
    }

    /// Delete API key usage data (called on key deletion)
    pub async fn delete_api_key_usage_data(&self, key_hash: &str) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let usage_key = format!("apikey:usage:{}", key_hash);
        let last_used_key = format!("apikey:last_used:{}", key_hash);

        let _: () = redis::cmd("DEL")
            .arg(&usage_key)
            .arg(&last_used_key)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    /// Update last_used timestamp in Redis (lightweight)
    pub async fn set_api_key_last_used(&self, key_hash: &str) -> Result<()> {
        let mut conn = self.conn.as_ref().clone();
        let redis_key = format!("apikey:last_used:{}", key_hash);
        let now = chrono::Utc::now().timestamp();

        // Store with 24h TTL (will be synced to DB periodically)
        let _: () = redis::cmd("SETEX")
            .arg(&redis_key)
            .arg(86400u64) // 24 hours
            .arg(now)
            .query_async(&mut conn)
            .await?;

        Ok(())
    }

    /// Get last_used timestamp from Redis
    pub async fn get_api_key_last_used(&self, key_hash: &str) -> Result<Option<i64>> {
        let mut conn = self.conn.as_ref().clone();
        let redis_key = format!("apikey:last_used:{}", key_hash);

        let timestamp: Option<i64> = redis::cmd("GET")
            .arg(&redis_key)
            .query_async(&mut conn)
            .await?;

        Ok(timestamp)
    }
}
