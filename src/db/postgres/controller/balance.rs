use anyhow::Result;

use std::sync::Arc;

use crate::{db::postgres::PostgresDatabase, types::response::HoldTokenResponse};

pub struct BalanceController {
    pub db: Arc<PostgresDatabase>,
}

impl BalanceController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        BalanceController { db }
    }
    pub async fn get_balances(&self, account_id: &str) -> Result<Vec<HoldTokenResponse>> {
        let balances = sqlx::query_as!(
            HoldTokenResponse,
            r#"
            SELECT 
                b.token_id,
                COALESCE(b.current_amount::text, '0') as amount,
                t.image_uri
            FROM 
                balance b
            LEFT JOIN 
                token t ON b.token_id = t.token_id
            WHERE 
                b.account_id = $1
            ORDER BY 
                b.current_amount DESC
            "#,
            account_id
        )
        .fetch_all(self.db.get_read_pool())
        .await?;
        Ok(balances)
    }
}
