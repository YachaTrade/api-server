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
                b.token_id as "token_id!: String",
                b.current_amount::text as "amount!: String",
                t.symbol as "symbol!: String",
                c.price::text as "price!: String",
                t.image_uri as "image_uri!: String"
            FROM 
                balance b
            JOIN 
                token t ON b.token_id = t.token_id
            JOIN
                curve c ON b.token_id = c.token_id
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
