use std::{env, str::FromStr, sync::Arc};

use alloy::{primitives::keccak256, signers::Signature};
use chrono::Utc;
use tokio::try_join;
use uuid::Uuid;

use crate::{
    config::{EXPIRATION_SESSION_KEY, MESSAGE_EXPIRATION},
    controllers::auth::{nonce::NonceController, session::SessionController},
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::auth::{AuthNonceRequest, AuthNonceResponse, AuthSessionRequest, AuthSessionResponse},
    utils::valid_account_id,
};

pub struct AuthService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl AuthService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn generate_nonce(
        &self,
        payload: AuthNonceRequest,
    ) -> Result<AuthNonceResponse, AppError> {
        let nonce = Uuid::new_v4().to_string();
        let address = valid_account_id(&payload.address)
            .ok_or_else(|| AppError::BadRequest("Invalid address".to_string()))?;

        let domain = env::var("APP_DOMAIN").unwrap_or_else(|_| "https://testnet.nad.fun".into());
        let chain_id = env::var("CHAIN_ID")
            .expect("CHAIN_ID must be set")
            .parse::<u64>()
            .unwrap();
        let issued_at = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let message = format!(
            "Account:\n\n{}\n\nURI: {}\n\nVersion: 1\n\nChain ID: {}\n\nNonce: {}\n\nIssued At: {}",
            address, domain, chain_id, nonce, issued_at
        );

        let nonce_controller = NonceController::new(self.postgres.clone());
        nonce_controller
            .set_nonce(&address, &message, *MESSAGE_EXPIRATION)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        Ok(AuthNonceResponse { nonce: message })
    }

    pub async fn create_session(
        &self,
        payload: AuthSessionRequest,
    ) -> Result<(AuthSessionResponse, String), AppError> {
        let env_chain_id = env::var("CHAIN_ID")
            .expect("CHAIN_ID must be set")
            .parse::<u64>()
            .unwrap();

        if payload.chain_id != env_chain_id {
            return Err(AppError::BadRequest(format!(
                "Invalid chain ID. Expected chain Id is {} your chain Id is {}",
                env_chain_id, payload.chain_id
            )));
        }

        let address = self
            .verify_wallet(&payload.signature, &payload.nonce)
            .await?;
        let redis = self.redis.clone();

        // Use atomic DELETE ... RETURNING to prevent nonce reuse attacks
        let nonce_controller = NonceController::new(self.postgres.clone());
        let sign_message = nonce_controller
            .get_and_delete_nonce(&address, &payload.nonce)
            .await
            .map_err(|_| AppError::Unauthorized("Invalid nonce".into()))?;

        if payload.nonce != sign_message {
            return Err(AppError::Unauthorized("Invalid nonce".into()));
        }

        let session_id = self.generate_session_id(address.as_str(), payload.nonce.as_str());

        let postgres = self.postgres.clone();
        let session_controller = SessionController::new(postgres.clone());

        let set_session_future = redis.set_session(&session_id, &address, *EXPIRATION_SESSION_KEY);

        let (account_info, _) = try_join!(
            async {
                session_controller
                    .set_session(&session_id, &address)
                    .await
                    .map_err(|err| AppError::InternalError(err.to_string()))
            },
            async {
                set_session_future
                    .await
                    .map_err(|err| AppError::RedisError(err.to_string()))
            }
        )?;

        Ok((AuthSessionResponse { account_info }, session_id))
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), AppError> {
        let redis_clone = self.redis.clone();
        let postgres_clone = self.postgres.clone();
        let session_controller = SessionController::new(postgres_clone);

        let (redis_result, postgres_result) = tokio::join!(
            async {
                redis_clone
                    .delete_session(session_id)
                    .await
                    .map_err(|err| AppError::RedisError(err.to_string()))
            },
            async {
                session_controller
                    .delete_session_by_id(session_id)
                    .await
                    .map_err(|err| AppError::InternalError(err.to_string()))
            },
        );

        redis_result?;
        postgres_result?;

        Ok(())
    }

    async fn verify_wallet(&self, signature: &str, message: &str) -> Result<String, AppError> {
        let signature = Signature::from_str(signature)
            .map_err(|err| AppError::BadRequest(format!("Invalid signature format: {}", err)))?;

        signature
            .recover_address_from_msg(message)
            .map(|address| address.to_string())
            .map_err(|err| {
                AppError::Unauthorized(format!("Failed to recover address from signature: {}", err))
            })
    }

    fn generate_session_id(&self, address: &str, message: &str) -> String {
        let uuid = Uuid::new_v4();
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();

        let combined = format!("{}-{}-{}-{}", address, timestamp, uuid, message);
        let hash = keccak256(combined.as_bytes());

        // keccak256 = 32 bytes = 64 hex chars (full entropy)
        hex::encode(hash)
    }
}
