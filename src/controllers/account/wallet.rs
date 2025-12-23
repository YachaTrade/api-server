use std::sync::Arc;

use anyhow::{Result, anyhow};
use sqlx::FromRow;

use crate::{db::postgres::PostgresDatabase, measure_postgres, types::account::GetWalletResponse};

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
        wallet: String,
    ) -> Result<GetWalletResponse> {
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
        .bind(&wallet)
        .fetch_one(self.db.get_write_pool());

        measure_postgres!("account_wallet.register_wallet", query)
            .map_err(|err| anyhow!("Failed to register wallet: {}", err))?;

        Ok(GetWalletResponse { account_id, wallet })
    }

    pub async fn get_wallet(&self, account_id: String) -> Result<GetWalletResponse> {
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

        Ok(GetWalletResponse {
            account_id: record.account_id,
            wallet: record.wallet,
        })
    }
}
