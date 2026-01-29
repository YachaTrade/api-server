use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::token::TokenController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    services::hackathon::HackathonService,
    types::token::TokenResponse,
};

pub struct TokenService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl TokenService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_token(&self, token_id: &str) -> Result<TokenResponse, AppError> {
        if let Ok(cached) = self.redis.get_token_response(token_id).await {
            return Ok(cached);
        }

        let controller = TokenController::new(self.postgres.clone());
        let hackathon_service = HackathonService::new(self.postgres.clone());

        // Fetch token and hackathon_info in parallel
        let (token_result, hackathon_info) = tokio::join!(
            controller.get_token(token_id),
            hackathon_service.get_hackathon_info(token_id)
        );

        let mut response = token_result.map_err(|err| {
            error!(
                "Failed to get token: token_id: {}, error: {}",
                token_id, err
            );
            AppError::NotFound(err.to_string())
        })?;

        // Inject hackathon_info
        response.token_info.hackathon_info = hackathon_info;

        if let Err(err) = self.redis.set_token_response(token_id, &response).await {
            error!("Failed to set token response: {}", err);
        }

        Ok(response)
    }
}
