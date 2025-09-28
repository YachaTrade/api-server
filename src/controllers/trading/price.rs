use std::sync::Arc;

use anyhow::{Result, anyhow};
use bigdecimal::BigDecimal;

use crate::{
    cache_key,
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::trading::price::PriceResponse,
    utils::single_flight::{GLOBAL_CACHE, with_cache},
};

pub struct PriceController {
    pub db: Arc<PostgresDatabase>,
}

impl PriceController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        PriceController { db }
    }

    pub async fn get_price(&self, token: &str) -> Result<PriceResponse> {
        let cache_key = cache_key!("price", token);

        let response = with_cache(&GLOBAL_CACHE.cache, &cache_key, || {
            let db = self.db.clone();
            let token = token.to_string();
            async move {
                let controller = PriceController::new(db);
                controller.fetch_price(&token).await
            }
        })
        .await?;

        Ok(response)
    }

    async fn fetch_price(&self, token: &str) -> Result<PriceResponse> {
        #[derive(sqlx::FromRow)]
        struct PriceRow {
            price: BigDecimal,
        }

        let price = measure_postgres!(
            "price.fetch_price",
            sqlx::query_as::<_, PriceRow>(
                r#"
                SELECT 
                    COALESCE(m.price, 0)::numeric as price
                FROM market m
                WHERE m.token_id = $1
                "#,
            )
            .bind(token)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch price: {}", err))?;

        Ok(PriceResponse {
            price: price.price.to_plain_string(),
            token_address: token.to_string(),
        })
    }
}
