use crate::{db::postgres::PostgresDatabase, measure_postgres};
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum Wallet {
    METAMASK,
    KEPLR,
    BACKPACK,
    HAHA,
    PHANTOM,
    RABBY,
    OKX,
    OTHER,
}

impl Wallet {
    pub fn to_string(&self) -> String {
        match self {
            Wallet::METAMASK => "METAMASK".to_string(),
            Wallet::KEPLR => "KEPLR".to_string(),
            Wallet::BACKPACK => "BACKPACK".to_string(),
            Wallet::HAHA => "HAHA".to_string(),
            Wallet::PHANTOM => "PHANTOM".to_string(),
            Wallet::RABBY => "RABBY".to_string(),
            Wallet::OKX => "OKX".to_string(),
            Wallet::OTHER => "OTHER".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterWalletRequest {
    pub wallet: Wallet,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AccountWalletResponse {
    pub account_id: String,
    pub wallet: Wallet,
}

#[derive(Debug, FromRow)]
struct WalletRow {
    account_id: String,
    wallet: String,
}

pub struct WalletController {
    db: Arc<PostgresDatabase>,
}

impl WalletController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn register_wallet(
        &self,
        account_id: String,
        wallet: Wallet,
    ) -> Result<AccountWalletResponse> {
        let query = sqlx::query_as::<_, WalletRow>(
            r#"
            INSERT INTO account_wallet (account_id, wallet)
            VALUES ($1, $2)
            ON CONFLICT (account_id) DO UPDATE
            SET wallet = EXCLUDED.wallet
            RETURNING account_id, wallet
            "#,
        )
        .bind(&account_id)
        .bind(wallet.to_string())
        .fetch_one(self.db.get_write_pool());

        measure_postgres!("account_wallet.register_wallet", query)
            .map_err(|err| anyhow!("Failed to register wallet: {}", err))?;

        Ok(AccountWalletResponse { account_id, wallet })
    }

    pub async fn get_wallet(&self, account_id: String) -> Result<AccountWalletResponse> {
        let query = sqlx::query_as::<_, WalletRow>(
            r#"
            SELECT account_id, wallet
            FROM account_wallet
            WHERE account_id = $1
            "#,
        )
        .bind(&account_id)
        .fetch_one(self.db.get_read_pool());

        let record = measure_postgres!("account_wallet.get_wallet", query)
            .map_err(|err| anyhow!("Failed to get wallet: {}", err))?;

        let wallet = match record.wallet.as_str() {
            "METAMASK" => Wallet::METAMASK,
            "KEPLR" => Wallet::KEPLR,
            "BACKPACK" => Wallet::BACKPACK,
            "HAHA" => Wallet::HAHA,
            "PHANTOM" => Wallet::PHANTOM,
            "RABBY" => Wallet::RABBY,
            "OKX" => Wallet::OKX,
            _ => Wallet::OTHER,
        };

        let response = AccountWalletResponse {
            account_id: record.account_id,
            wallet,
        };

        Ok(response)
    }
}
