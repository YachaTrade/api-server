use std::env;

use redis::{pipe, AsyncCommands, Client};
use tracing::{debug, info};

use anyhow::Result;

use crate::{
    config::{NONCE_EXPIRATION, ORDER_EXPIRATION, QUERY_EXPIRATION, SEARCH_EXPIRATION},
    types::{
        common::pagination::PaginationParams,
        search::{SearchAccountResponse, SearchResponse, SearchTokenResponse},
        token::order::{OrderMessage, TokenOrderType},
    },
};

pub struct RedisDatabase {
    pub client: Client,
}

impl RedisDatabase {
    pub async fn new() -> Self {
        let client = {
            let redis_url = env::var("REDIS_URL").expect("REDIS_URL must be set");
            debug!("Connecting to standalone Redis at: {}", redis_url);
            let client = Client::open(redis_url).unwrap();
            client
        };

        RedisDatabase { client }
    }

    //session

    //nonce -> address -> nonce
    pub async fn set_nonce(&self, address: &str, nonce: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        let key = format!("session:{}:nonce", address);

        conn.set_ex::<String, String, ()>(key, nonce.to_string(), *NONCE_EXPIRATION)
            .await?;

        Ok(())
    }

    pub async fn get_nonce(&self, address: &str) -> Result<String> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        let key = format!("session:{}:nonce", address);
        let nonce: Option<String> = conn.get(key).await?;
        info!("Nonce for address {}: {:?}", address, nonce);
        match nonce {
            Some(nonce) => Ok(nonce),
            None => Err(anyhow::anyhow!("Nonce not found")),
        }
    }

    pub async fn del_nonce(&self, address: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;

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
        let mut conn = self.client.get_multiplexed_async_connection().await?;

        let key = format!("session:{}:id", session_id);
        conn.set_ex::<_, _, ()>(key, address, expiration).await?;
        debug!(
            "Session set: {} -> {} (expires in {}s)",
            session_id, address, expiration
        );
        Ok(())
    }

    pub async fn get_address_by_session(&self, session_id: &str) -> Result<String> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("session:{}:id", session_id);
        let address = conn.get::<_, String>(key).await?;
        debug!("Address for session {}: {:?}", session_id, address);
        Ok(address)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("session:{}:id", session_id);
        conn.del::<_, ()>(key).await?;
        debug!("Session deleted: {}", session_id);
        Ok(())
    }
}

//search response
impl RedisDatabase {
    pub async fn set_search_response(&self, query: &str, response: &SearchResponse) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let token_key = format!("search:{}:tokens", query);
        let account_key = format!("search:{}:accounts", query);

        // Serialize token and account responses separately
        let token_json = serde_json::to_string(&response.tokens)?;
        let account_json = serde_json::to_string(&response.accounts)?;

        // Use pipeline to set both values atomically
        let mut pipe = redis::pipe();
        pipe.set_ex::<_, _>(token_key, token_json, *SEARCH_EXPIRATION)
            .set_ex::<_, _>(account_key, account_json, *SEARCH_EXPIRATION);

        let _: ((), ()) = pipe.query_async(&mut conn).await?;
        debug!("Search response set for query {}", query);
        Ok(())
    }

    pub async fn get_search_response(
        &self,
        query: &str,
        pagination: PaginationParams,
    ) -> Result<Option<SearchResponse>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let token_key = format!("search:{}:tokens", query);
        let account_key = format!("search:{}:accounts", query);

        // Get both token and account responses
        let (token_json, account_json): (Option<String>, Option<String>) = redis::pipe()
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
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("order:{}:response", order_type.as_str());
        let response_json = serde_json::to_string(response)?;
        //pset is miliseconds
        conn.pset_ex::<_, _, ()>(key, response_json, *ORDER_EXPIRATION)
            .await?;
        debug!("Order response set for order type {:?}", order_type);
        Ok(())
    }

    pub async fn get_order_response(&self, order_type: &TokenOrderType) -> Result<OrderMessage> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("order:{}:response", order_type.as_str());
        let response_json: String = conn.get(key).await?;
        let response: OrderMessage = serde_json::from_str(&response_json)?;
        debug!("Order response retrieved for order type {:?}", order_type);
        Ok(response)
    }
}
