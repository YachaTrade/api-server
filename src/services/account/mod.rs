use std::sync::Arc;

use tracing::warn;

use crate::{
    controllers::account::{AccountController, wallet::WalletController, x::AccountXController},
    db::postgres::PostgresDatabase,
    result::AppError,
    types::account::{
        AccountResponse, UpdateAccountRequest,
        wallet::{AccountWalletResponse, RegisterWalletRequest},
        x::{
            ConnectXRequest, ConnectedXAccountResponse, DisconnectXRequest,
            DisconnectedXAccountResponse, GetXHandleResponse, UpdateXRequest,
        },
    },
};

pub struct AccountService {
    db: Arc<PostgresDatabase>,
}

impl AccountService {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn update_account(
        &self,
        session_address: &str,
        payload: UpdateAccountRequest,
    ) -> Result<AccountResponse, AppError> {
        let UpdateAccountRequest {
            nickname,
            bio,
            image_uri,
        } = payload;

        if nickname.is_none() && bio.is_none() && image_uri.is_none() {
            warn!(
                "Update account Error: At least one of nickName or bio, or image must be provided"
            );
            return Err(AppError::BadRequest(
                "At least one of nickName, bio, or image must be provided".into(),
            ));
        }

        if let Some(ref uri) = image_uri {
            if !uri.starts_with("https://storage.nadapp.net/profile/") {
                warn!("Update account Error: Invalid image URL");
                return Err(AppError::BadRequest("Invalid image URL".into()));
            }
        }

        if let Some(ref nickname_value) = nickname {
            if nickname_value.starts_with('@') || nickname_value.starts_with('#') {
                warn!("Update account Error: Nickname cannot start with @ or #");
                return Err(AppError::BadRequest(
                    "Nickname cannot start with @ or #".into(),
                ));
            }
        }

        let clean_text =
            |text: Option<String>| text.map(|t| t.trim().to_string()).filter(|t| !t.is_empty());

        let controller = AccountController::new(self.db.clone());
        let updated_account = controller
            .update_account(
                session_address,
                clean_text(image_uri),
                clean_text(nickname),
                clean_text(bio),
            )
            .await
            .map_err(|err| {
                warn!("Update account Error {:?}", err);
                AppError::InternalError(err.to_string())
            })?;

        Ok(AccountResponse {
            account: updated_account,
        })
    }

    pub async fn get_account(&self, session_address: &str) -> Result<AccountResponse, AppError> {
        let controller = AccountController::new(self.db.clone());
        let account = controller
            .get_account(session_address)
            .await
            .map_err(|err| {
                warn!("get account Error {:?}", err);
                AppError::BadRequest(err.to_string())
            })?;

        Ok(AccountResponse { account })
    }

    pub async fn connect_x(
        &self,
        session_address: &str,
        payload: ConnectXRequest,
    ) -> Result<ConnectedXAccountResponse, AppError> {
        payload
            .validate()
            .map_err(|err| AppError::BadRequest(err.to_string()))?;

        let controller = AccountXController::new(self.db.clone());
        controller
            .connect_x(session_address, payload)
            .await
            .map_err(|err| {
                warn!("connect x account Error {:?}", err);
                AppError::BadRequest(err.to_string())
            })
    }

    pub async fn disconnect_x(
        &self,
        session_address: String,
    ) -> Result<DisconnectedXAccountResponse, AppError> {
        let controller = AccountXController::new(self.db.clone());
        controller
            .disconnect_x(session_address)
            .await
            .map_err(|err| {
                warn!("disconnect x account Error {:?}", err);
                let message = err.to_string();
                if message.contains("not found") {
                    AppError::NotFound(message)
                } else {
                    AppError::BadRequest(message)
                }
            })
    }

    pub async fn update_x(
        &self,
        session_address: String,
        payload: UpdateXRequest,
    ) -> Result<GetXHandleResponse, AppError> {
        payload
            .validate()
            .map_err(|err| AppError::BadRequest(err.to_string()))?;

        let controller = AccountXController::new(self.db.clone());
        controller
            .update_x(session_address, payload.x_image_uri)
            .await
            .map_err(|err| {
                warn!("update x Error {:?}", err);
                AppError::BadRequest(err.to_string())
            })
    }

    pub async fn register_wallet(
        &self,
        session_address: String,
        payload: RegisterWalletRequest,
    ) -> Result<AccountWalletResponse, AppError> {
        let controller = WalletController::new(self.db.clone());
        controller
            .register_wallet(session_address, payload.wallet)
            .await
            .map_err(|err| {
                warn!("register wallet Error {:?}", err);
                AppError::BadRequest(err.to_string())
            })
    }

    pub async fn get_wallet(
        &self,
        session_address: String,
    ) -> Result<AccountWalletResponse, AppError> {
        let controller = WalletController::new(self.db.clone());
        controller.get_wallet(session_address).await.map_err(|err| {
            warn!("get wallet Error {:?}", err);
            AppError::BadRequest(err.to_string())
        })
    }
}
