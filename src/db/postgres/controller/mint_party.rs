use crate::{
    db::postgres::{model::MintParty, PostgresDatabase},
    types::{
        order_type::{MintPartyOrderType, OrderDirection},
        response::{
            MintPartyBalance, MintPartyBalanceRaw, MintPartyClaimStatus, MintPartyDepositList,
            MintPartyDepositListRaw, MintPartyRaw, MintPartyResponse,
        },
    },
};
use anyhow::{anyhow, Result};
use tracing::info;

use std::sync::Arc;

pub struct MintPartyController {
    pub db: Arc<PostgresDatabase>,
}

impl MintPartyController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        MintPartyController { db }
    }

    pub async fn get_mint_party_tx(&self, tx: String) -> Result<MintParty> {
        info!("tx: {}", tx);
        let mint_party = sqlx::query_as!(
            MintParty,
            r#"
            SELECT * FROM mint_party WHERE transaction_hash = $1
            "#,
            tx
        )
        .fetch_one(self.db.get_read_pool())
        .await
        .map_err(|err| anyhow!("Fail get mint_party Reason :{err}"))?;

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
        // Single query to check and update atomically
        let mint_party = sqlx::query_as!(
            MintParty,
            r#"
            UPDATE mint_party 
            SET description = $1,
                twitter = $2,
                telegram = $3,
                website = $4,
                is_updated = true
            WHERE transaction_hash = $5 
                AND account_id = $6
                AND is_updated = false 
            RETURNING *
            "#,
            description,
            twitter,
            telegram,
            website,
            transaction_hash,
            creator
        )
        .fetch_optional(&self.db.write_pool)
        .await
        .map_err(|err| anyhow!("Failed to update mint party. Reason: {}", err))?
        .ok_or_else(|| anyhow!("Mint Party is already updated"))?;

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

    pub async fn get_mint_party_balance(
        &self,
        account_id: String,
    ) -> Result<Vec<MintPartyBalance>> {
        let raw_balances = sqlx::query_as!(
            MintPartyBalanceRaw,
            r#"
            SELECT 
                
                a.account_id,
                a.nickname as account_nickname,
                a.image_uri as account_image_uri,
                mp.mint_party_id,
                mp.name as mint_party_name,
                mp.symbol as mint_party_symbol,
                mp.image_uri as mint_party_image_uri,
                mb.amount,
                mb.claim_status as "claim_status: MintPartyClaimStatus",
                mb.created_at,
                mb.is_claimable,
                mb.transaction_hash,
                mb.token_id
            FROM mint_party_balance mb
            INNER JOIN mint_party mp ON mb.mint_party_id = mp.mint_party_id
            INNER JOIN account a ON mb.account_id = a.account_id
            WHERE mb.account_id = $1
            ORDER BY mb.created_at DESC
            "#,
            account_id
        )
        .fetch_all(&self.db.read_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch mint party balances: {}", e))?;

        Ok(raw_balances.into_iter().map(Into::into).collect())
    }

    pub async fn get_mint_party_deposit_list(
        &self,
        account_id: String,
    ) -> Result<Vec<MintPartyDepositList>> {
        info!("account_id: {}", account_id);
        let raw_list = sqlx::query_as!(
            MintPartyDepositListRaw,
            r#"
            WITH active_mint_parties AS (
                SELECT mint_party_id 
                FROM mint_party 
                WHERE account_id = $1 
                AND is_closed = false 
                
            )
            SELECT 
                a.account_id,
                a.nickname as account_nickname,
                a.image_uri as account_image_uri,
                mpd.comment,
                mp.mint_party_id,
                mp.name as mint_party_name,
                mp.symbol as mint_party_symbol,
                mp.image_uri as mint_party_image_uri,
                mpd.created_at,
                mp.funding_amount as amount,
                mpd.is_white_list,
                mpd.transaction_hash
            FROM mint_party_deposit_list mpd
            INNER JOIN mint_party mp ON mpd.mint_party_id = mp.mint_party_id
            INNER JOIN account a ON mpd.account_id = a.account_id
            INNER JOIN active_mint_parties amp ON mpd.mint_party_id = amp.mint_party_id
            ORDER BY mpd.created_at DESC
            "#,
            account_id
        )
        .fetch_all(&self.db.read_pool)
        .await
        .map_err(|e| anyhow!("Failed to fetch mint party deposits: {}", e))?;

        Ok(raw_list.into_iter().map(Into::into).collect())
    }
}
