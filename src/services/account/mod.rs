use std::sync::Arc;

use crate::{
    controllers::account::{wallet::WalletController, x::AccountXController, AccountController},
    db::postgres::PostgresDatabase,
    result::AppError,
    types::account::{
        AccountResponse, ConnectXRequest, GetWalletResponse, RegisterWalletRequest,
        UpdateAccountRequest, UpdateXRequest,
    },
};

pub struct AccountService {
    postgres: Arc<PostgresDatabase>,
}

impl AccountService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    pub async fn get_account(&self, account_id: &str) -> Result<AccountResponse, AppError> {
        let controller = AccountController::new(self.postgres.clone());
        let account_info = controller
            .get_account(account_id)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;

        Ok(AccountResponse { account_info })
    }

    pub async fn update_account(
        &self,
        account_id: &str,
        req: UpdateAccountRequest,
    ) -> Result<AccountResponse, AppError> {
        let controller = AccountController::new(self.postgres.clone());
        let account_info = controller
            .update_account(account_id, req.image_uri, req.nickname, req.bio)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;

        Ok(AccountResponse { account_info })
    }

    pub async fn connect_x(
        &self,
        account_id: &str,
        req: ConnectXRequest,
    ) -> Result<AccountResponse, AppError> {
        let controller = AccountXController::new(self.postgres.clone());
        let account_info = controller
            .connect_x(account_id, req)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;

        Ok(AccountResponse { account_info })
    }

    pub async fn disconnect_x(&self, account_id: String) -> Result<AccountResponse, AppError> {
        let controller = AccountXController::new(self.postgres.clone());
        let account_info = controller
            .disconnect_x(account_id)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;

        Ok(AccountResponse { account_info })
    }

    pub async fn update_x(
        &self,
        account_id: String,
        req: UpdateXRequest,
    ) -> Result<AccountResponse, AppError> {
        let controller = AccountXController::new(self.postgres.clone());
        let account_info = controller
            .update_x(account_id, req)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;

        Ok(AccountResponse { account_info })
    }

    pub async fn register_wallet(
        &self,
        account_id: String,
        req: RegisterWalletRequest,
    ) -> Result<AccountResponse, AppError> {
        let controller = WalletController::new(self.postgres.clone());
        controller
            .register_wallet(account_id.clone(), req.wallet)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;

        // Return updated account info
        self.get_account(&account_id).await
    }

    pub async fn get_wallet(&self, account_id: String) -> Result<GetWalletResponse, AppError> {
        let controller = WalletController::new(self.postgres.clone());
        let response = controller
            .get_wallet(account_id)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;

        Ok(response)
    }
}
