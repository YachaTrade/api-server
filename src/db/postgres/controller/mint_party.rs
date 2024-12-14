use crate::{
    db::postgres::{model::MintParty, PostgresDatabase},
    types::{
        order_type::{MintPartyOrderType, OrderDirection},
        response::{AccountInfo, MintPartyResponse},
    },
};
use anyhow::{anyhow, Result};
use bigdecimal::BigDecimal;
use sqlx::FromRow;
use std::sync::Arc;
#[derive(FromRow)]
struct MintPartyRaw {
    // Account fields
    account_id: String,
    nickname: String,
    account_image_uri: String,
    // MintParty fields
    mint_party_id: String,
    name: String,
    symbol: String,
    description: Option<String>,
    image_uri: String,
    current_white_list_count: i16,
    allow_white_list_count: i16,
    funding_amount: BigDecimal,
    total_deposit_amount: BigDecimal,
}

impl From<MintPartyRaw> for MintPartyResponse {
    fn from(row: MintPartyRaw) -> Self {
        Self {
            account_info: AccountInfo {
                account_id: row.account_id,
                nickname: row.nickname,
                image_uri: row.account_image_uri,
            },
            mint_party_id: row.mint_party_id,
            name: row.name,
            symbol: row.symbol,
            description: row.description.unwrap_or_default(),
            image_uri: row.image_uri,
            current_white_list_count: row.current_white_list_count.to_string(),
            allow_white_list_count: row.allow_white_list_count.to_string(),
            funding_amount: row.funding_amount.to_string(),
            total_deposit_amount: row.total_deposit_amount.to_string(),
        }
    }
}

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

    pub async fn get_last_join_mint_party(&self) -> Result<Vec<MintPartyResponse>> {
        const LIMIT: i64 = 5;

        let rows = sqlx::query_as!(
            MintPartyRaw,
            r#"
            SELECT 
                a.account_id,
                a.nickname,
                a.image_uri as account_image_uri,
                m.mint_party_id,
                m.name,
                m.symbol,
                m.description,
                m.image_uri,
                m.current_white_list_count,
                m.allow_white_list_count,
                m.funding_amount,
                m.total_deposit_amount
            FROM mint_party m
            JOIN account a ON m.account_id = a.account_id
            WHERE m.is_closed = false 
              AND m.is_finished = false
            ORDER BY 
                m.total_deposit_amount DESC,
                (m.allow_white_list_count - m.current_white_list_count) ASC
            LIMIT $1
            "#,
            LIMIT
        )
        .fetch_all(&self.db.read_pool)
        .await
        .map_err(|err| anyhow!("Failed to get last join mint party: {err}"))?;

        Ok(rows.into_iter().map(MintPartyResponse::from).collect())
    }

    pub async fn get_mint_partys(
        &self,
        order_type: MintPartyOrderType,
        direction: OrderDirection,
        page: Option<i16>,
    ) -> Result<Vec<MintPartyResponse>> {
        const ITEMS_PER_PAGE: i64 = 6;

        let offset = match page {
            Some(p) if p >= 0 => p as i64 * ITEMS_PER_PAGE,
            _ => 0,
        };

        let base_query = r#"
            SELECT 
                a.account_id,
                a.nickname,
                a.image_uri as account_image_uri,
                m.mint_party_id,
                m.name,
                m.symbol,
                m.description,
                m.image_uri,
                m.current_white_list_count,
                m.allow_white_list_count,
                m.funding_amount,
                m.total_deposit_amount
            FROM mint_party m
            JOIN account a ON m.account_id = a.account_id
            WHERE m.is_closed = false 
              AND m.is_finished = false
        "#;

        let order_by = match (order_type, direction) {
            (MintPartyOrderType::CreationTime, OrderDirection::Asc) => "ORDER BY m.created_at ASC",
            (MintPartyOrderType::CreationTime, OrderDirection::Desc) => {
                "ORDER BY m.created_at DESC"
            }
            (MintPartyOrderType::FundingAmount, OrderDirection::Asc) => {
                "ORDER BY m.total_deposit_amount ASC"
            }
            (MintPartyOrderType::FundingAmount, OrderDirection::Desc) => {
                "ORDER BY m.total_deposit_amount DESC"
            }
        };

        let query = format!(
            "{} {} LIMIT {} OFFSET {}",
            base_query, order_by, ITEMS_PER_PAGE, offset
        );

        let rows = sqlx::query_as::<_, MintPartyRaw>(&query)
            .fetch_all(&self.db.read_pool)
            .await
            .map_err(|err| anyhow!("Failed to get mint party list: {err}"))?;

        Ok(rows.into_iter().map(MintPartyResponse::from).collect())
    }
}
