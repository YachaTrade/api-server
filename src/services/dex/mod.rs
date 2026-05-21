use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::dex::{
        pool::PoolController, position::PositionController, tokens::TokensController,
    },
    db::postgres::PostgresDatabase,
    result::AppError,
    types::dex::{
        pool::PoolDetailResponse,
        position::LpPositionsResponse,
        tokens::{DexTokenListQuery, DexTokenListResponse},
    },
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

    pub async fn get_pool_detail(
        &self,
        pool_id: &str,
    ) -> Result<Option<PoolDetailResponse>, AppError> {
        let controller = PoolController::new(self.postgres.clone());
        controller.get_pool_detail(pool_id).await.map_err(|err| {
            error!(
                "Failed to get pool detail: pool_id={}, error={}",
                pool_id, err
            );
            AppError::InternalError(err.to_string())
        })
    }

    pub async fn get_tokens(
        &self,
        query: &DexTokenListQuery,
    ) -> Result<DexTokenListResponse, AppError> {
        let controller = TokensController::new(self.postgres.clone());
        controller.list_tokens(query).await.map_err(|err| {
            error!("Failed to list dex tokens: error={}", err);
            AppError::InternalError(err.to_string())
        })
    }
}
