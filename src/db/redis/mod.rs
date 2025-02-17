use std::env;

use redis::{AsyncCommands, Client};
use tracing::{debug, info};

use anyhow::Result;

use crate::{
    config::{NONCE_EXPIRATION, ORDER_EXPIRATION, QUERY_EXPIRATION},
    types::token::order::{OrderMessage, SearchResponse, TokenOrderType},
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
        let key = format!("search:{}:response", query);

        // SearchResponse를 JSON 문자열로 직렬화
        let response_json = serde_json::to_string(response)?;

        conn.set_ex::<_, _, ()>(key, response_json, *QUERY_EXPIRATION)
            .await?;
        debug!("Search response set for query {}", query);
        Ok(())
    }

    pub async fn get_search_response(&self, query: &str) -> Result<SearchResponse> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("search:{}:response", query);

        // Redis에서 JSON 문자열 가져오기
        let response_json: String = conn.get(key).await?;

        // JSON 문자열을 SearchResponse로 역직렬화
        let response: SearchResponse = serde_json::from_str(&response_json)?;

        debug!("Search response retrieved for query {}", query);
        Ok(response)
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
