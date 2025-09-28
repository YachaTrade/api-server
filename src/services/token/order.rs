use std::sync::Arc;

use tracing::warn;

use crate::{
    controllers::token::order::OrderController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::{
        common::pagination::PaginationParams,
        token::order::{OrderMessage, TokenOrderType},
    },
};

pub struct TokenOrderService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl TokenOrderService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn get_order(
        &self,
        order_type: TokenOrderType,
        pagination: &PaginationParams,
    ) -> Result<OrderMessage, AppError> {
        if let Ok(cached) = self
            .redis
            .get_order_response(&order_type, Some(pagination))
            .await
        {
            return Ok(cached);
        }

        let controller = OrderController::new(self.postgres.clone());

        let order_tokens = controller
            .get_order_tokens(order_type, pagination)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        let king_of_the_hill = controller
            .get_latest_king_of_the_hill()
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        let total_count = controller
            .get_total_count_by_type(&order_type)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))?;

        let response = OrderController::build_order_message(
            order_type,
            Some(order_tokens),
            king_of_the_hill,
            total_count,
        );

        if let Err(err) = self
            .redis
            .set_order_response(&order_type, &response, Some(pagination))
            .await
        {
            warn!("Failed to set {:?} cache: {}", order_type, err);
        }

        Ok(response)
    }
}
