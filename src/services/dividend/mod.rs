use std::sync::Arc;

use tracing::error;

use crate::{
    controllers::dividend::DividendController,
    db::postgres::PostgresDatabase,
    result::AppError,
    types::{
        common::pagination::PaginationParams,
        dex::tokens::DexTokenListResponse,
        dividend::{
            DividendHoldersResponse, DividendTokenQuery, DividendTokensResponse,
            DividendVaultResponse,
        },
    },
};

pub struct DividendService {
    postgres: Arc<PostgresDatabase>,
}

impl DividendService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    /// ① Profile Dividend — a wallet's dividend-bearing tokens.
    pub async fn get_profile_dividends(
        &self,
        account_id: &str,
        pagination: &PaginationParams,
    ) -> Result<DividendTokensResponse, AppError> {
        let controller = DividendController::new(self.postgres.clone());
        controller
            .get_profile_dividends(account_id, pagination)
            .await
            .map_err(|err| {
                error!(
                    "Failed to get profile dividends: account_id: {}, error: {}",
                    account_id, err
                );
                AppError::InternalError(err.to_string())
            })
    }

    /// ② Trade Dividend — holder ranking for a token.
    pub async fn get_dividend_holders(
        &self,
        token_id: &str,
        pagination: &PaginationParams,
    ) -> Result<DividendHoldersResponse, AppError> {
        let controller = DividendController::new(self.postgres.clone());
        controller
            .get_dividend_holders(token_id, pagination)
            .await
            .map_err(|err| {
                error!(
                    "Failed to get dividend holders: token_id: {}, error: {}",
                    token_id, err
                );
                AppError::InternalError(err.to_string())
            })
    }

    /// ③ Vault Dividend — dividend config/summary card for a token.
    pub async fn get_dividend_vault(
        &self,
        token_id: &str,
    ) -> Result<DividendVaultResponse, AppError> {
        let controller = DividendController::new(self.postgres.clone());
        controller
            .get_dividend_vault(token_id)
            .await
            .map_err(|err| {
                error!(
                    "Failed to get dividend vault: token_id: {}, error: {}",
                    token_id, err
                );
                AppError::InternalError(err.to_string())
            })
    }

    /// Dividend token search — candidate payout tokens (whitelist ∪ V1-graduated ∪ V2).
    pub async fn get_dividend_tokens(
        &self,
        query: &DividendTokenQuery,
    ) -> Result<DexTokenListResponse, AppError> {
        let controller = DividendController::new(self.postgres.clone());
        controller
            .get_dividend_tokens(query)
            .await
            .map_err(|err| {
                error!("Failed to get dividend tokens: error: {}", err);
                AppError::InternalError(err.to_string())
            })
    }
}
