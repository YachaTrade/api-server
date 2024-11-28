use crate::db::postgres::{model::MintParty, PostgresDatabase};
use anyhow::{anyhow, Result};
use std::sync::Arc;

pub struct MintPartyController {
    pub db: Arc<PostgresDatabase>,
}

impl MintPartyController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        MintPartyController { db }
    }

    pub async fn get_mint_party_tx(&self, tx: String) -> Result<MintParty> {
        let mint_party = sqlx::query_as!(
            MintParty,
            r#"
            SELECT * FROM mint_party WHERE transaction_hash = $1
            "#,
            tx
        )
        .fetch_one(self.db.get_read_pool())
        .await?;

        Ok(mint_party)
    }
    pub async fn update_mint_party_metadata(
        &self,
        transaction_hash: String,
        description: String,
        twitter: Option<String>,
        telegram: Option<String>,
        website: Option<String>,
        creator: String,
    ) -> Result<MintParty> {
        let mint_party = sqlx::query_as!(
            MintParty,
            r#"
            UPDATE mint_party
            SET description = $1,
                twitter = $2,
                telegram = $3,
                website = $4,
                is_updated = true
            WHERE transaction_hash = $5 AND account_id = $6
            RETURNING *
            "#,
            description,
            twitter,
            telegram,
            website,
            transaction_hash,
            creator
        )
        .fetch_one(&self.db.write_pool)
        .await
        .map_err(|err| anyhow!("Fail update mint_party Reason :{err}"))?;

        Ok(mint_party)
    }

    // pub async fn get_account_mint_party_balance(&self,account_id:String)->Result<{

    // }
}
