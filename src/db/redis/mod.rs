use redis::{Client, Commands};
use tracing::debug;

use anyhow::Result;

use crate::{constant::NONCE_EXPIRATION, env};

pub struct RedisDatabase {
    pub client: Client,
}
impl RedisDatabase {
    pub async fn new() -> Self {
        let client = {
            let host = env::get_env("REDIS_HOST");
            let port = env::get_env("REDIS_PORT");
            let connection_string = format!("redis://{}:{}", host, port);
            debug!("Connecting to standalone Redis at: {}", connection_string);
            let client = Client::open(connection_string).unwrap();
            client
        };

        RedisDatabase { client }
    }

    //nonce -> address -> nonce
    pub async fn set_nonce(&self, address: &str, nonce: &str) -> Result<()> {
        let mut con = self.client.get_connection()?;
        let key = format!("session:{}:nonce", address);

        con.set_ex::<String, String, ()>(key, nonce.to_string(), *NONCE_EXPIRATION)?;
        debug!("Nonce set for address {}: {}", address, nonce);
        Ok(())
    }

    pub async fn get_nonce(&self, address: &str) -> Result<String> {
        let mut con = self.client.get_connection()?;
        let key = format!("session:{}:nonce", address);
        let nonce: Option<String> = con.get(key)?;
        debug!("Nonce for address {}: {:?}", address, nonce);
        match nonce {
            Some(nonce) => Ok(nonce),
            None => Err(anyhow::anyhow!("Nonce not found")),
        }
    }
    pub async fn del_nonce(&self, address: &str) -> Result<()> {
        let mut con = self.client.get_connection()?;
        let key = format!("session:{}:nonce", address);
        con.del::<_, ()>(key)?;
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
        let mut con = self.client.get_connection()?;
        let key = format!("session:{}:id", session_id);
        con.set_ex::<_, _, ()>(key, address, expiration)?;
        debug!(
            "Session set: {} -> {} (expires in {}s)",
            session_id, address, expiration
        );
        Ok(())
    }

    pub async fn get_address_by_session(&self, session_id: &str) -> Result<String> {
        let mut con = self.client.get_connection()?;
        let key = format!("session:{}:id", session_id);
        let address = con.get::<_, String>(key)?;
        debug!("Address for session {}: {:?}", session_id, address);
        Ok(address)
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let mut con = self.client.get_connection()?;
        let key = format!("session:{}:id", session_id);
        con.del::<_, ()>(key)?;
        debug!("Session deleted: {}", session_id);
        Ok(())
    }
}
