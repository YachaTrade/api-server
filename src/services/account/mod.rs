use std::{str::FromStr, sync::Arc};

use alloy::primitives::Address;

use crate::{
    controllers::account::{AccountController, wallet::WalletController, x::AccountXController},
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::account::{
        AccountResponse, ConnectXRequest, GetWalletResponse, RegisterWalletRequest,
        UpdateAccountRequest, UpdateXRequest,
    },
};

pub struct AccountService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl AccountService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
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
        // Validate nickname: cannot start with @
        if let Some(ref nickname) = req.nickname
            && nickname.starts_with('@')
        {
            return Err(AppError::BadRequest(
                "Nickname cannot start with '@'".to_string(),
            ));
        }

        let controller = AccountController::new(self.postgres.clone());
        let account_info = controller
            .update_account(account_id, req.image_uri, req.nickname, req.bio)
            .await
            .map_err(|e| AppError::InternalError(e.to_string()))?;

        // Cache updated account info in Redis
        self.redis
            .set_account_info(account_id, &account_info)
            .await
            .ok(); // Ignore cache errors, don't fail the request

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
        // Validate wallet is a valid EVM address
        Address::from_str(&req.wallet)
            .map_err(|_| AppError::BadRequest("Invalid wallet address format".to_string()))?;

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
