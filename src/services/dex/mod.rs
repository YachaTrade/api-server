use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::dex::position::PositionController, db::postgres::PostgresDatabase,
    result::AppError, types::dex::position::LpPositionsResponse,
};

pub struct DexService {
    postgres: Arc<PostgresDatabase>,
}

impl DexService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    pub async fn get_positions(&self, account_id: &str) -> Result<LpPositionsResponse, AppError> {
        let controller = PositionController::new(self.postgres.clone());
        controller.get_positions(account_id).await.map_err(|err| {
            error!(
                "Failed to get LP positions: account_id={}, error={}",
                account_id, err
            );
            AppError::InternalError(err.to_string())
        })
    }
}
