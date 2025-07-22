use crate::db::postgres::PostgresDatabase;
use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::info;
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
        let start_time = Instant::now();

        let query = sqlx::query!(
            r#"
            INSERT INTO account_wallet (account_id, wallet)
            VALUES ($1, $2)
            ON CONFLICT (account_id) DO UPDATE
            SET wallet = EXCLUDED.wallet
            RETURNING account_id, wallet
            "#,
            account_id,
            wallet.to_string()
        )
        .fetch_one(self.db.get_write_pool());

        tokio::time::timeout(Duration::from_millis(500), query)
            .await
            .map_err(|_| anyhow!("Query timeout after 500ms"))?
            .map_err(|err| anyhow!("Failed to register wallet: {}", err))?;

        let elapsed = start_time.elapsed();
        info!(
            "register_wallet completed in {:?} for account_id: {}",
            elapsed, account_id
        );

        Ok(AccountWalletResponse { account_id, wallet })
    }

    pub async fn get_wallet(&self, account_id: String) -> Result<AccountWalletResponse> {
        let start_time = Instant::now();

        let query = sqlx::query!(
            r#"
            SELECT account_id, wallet
            FROM account_wallet
            WHERE account_id = $1
            "#,
            account_id
        )
        .fetch_one(self.db.get_read_pool());

        let record = tokio::time::timeout(Duration::from_millis(500), query)
            .await
            .map_err(|_| anyhow!("Query timeout after 500ms"))?
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

        let elapsed = start_time.elapsed();
        info!(
            "get_wallet completed in {:?} for account_id: {}",
            elapsed, account_id
        );

        Ok(response)
    }
}
