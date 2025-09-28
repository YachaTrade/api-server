use std::sync::Arc;

use crate::{
    controllers::trading::price::PriceController, db::postgres::PostgresDatabase, result::AppError,
    types::trading::price::PriceResponse,
};

pub struct PriceService {
    postgres: Arc<PostgresDatabase>,
}

impl PriceService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    pub async fn get_price(&self, token: &str) -> Result<PriceResponse, AppError> {
        let controller = PriceController::new(self.postgres.clone());
        controller
            .get_price(token)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))
    }
}
