use std::sync::Arc;

use anyhow::{Result, anyhow};
use sqlx::types::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::vault::{
        BurnStats, CreatorFeeStats, EmptyStats, GiftStats, LpStats, TokenVaultsResponse,
        VaultEntry, VaultStats, VaultType,
    },
};

#[derive(Debug, sqlx::FromRow)]
struct VaultRow {
    vault_id: String,
    bps: i32,
    name: String,
    vault_type: VaultType,
    active: bool,

    // BURN — `v2_burn_vault_stats`
    burn_quote_spent: Option<BigDecimal>,
    burn_tokens_burned: Option<BigDecimal>,
    burn_count: Option<i32>,
    burn_updated_at: Option<i64>,

    // LP — `v2_lp_vault_stats`
    lp_quote_injected: Option<BigDecimal>,
    lp_token_injected: Option<BigDecimal>,
    lp_lp_burned: Option<BigDecimal>,
    lp_inject_count: Option<i32>,
    lp_updated_at: Option<i64>,

    // CREATOR_FEE — `v2_creator_fee_vault_stats`
    cf_current_balance: Option<BigDecimal>,
    cf_total_deposited: Option<BigDecimal>,
    cf_total_claimed: Option<BigDecimal>,
    cf_deposit_count: Option<i32>,
    cf_claim_count: Option<i32>,
    cf_updated_at: Option<i64>,

    // GIFT — `v2_gift_vault_stats`
    gift_current_state: Option<String>,
    gift_current_balance: Option<BigDecimal>,
    gift_total_deposited: Option<BigDecimal>,
    gift_total_claimed: Option<BigDecimal>,
    gift_total_expired: Option<BigDecimal>,
    gift_platform: Option<String>,
    gift_platform_id: Option<String>,
    gift_receiver: Option<String>,
    gift_buyback_quote_spent: Option<BigDecimal>,
    gift_buyback_tokens: Option<BigDecimal>,
    gift_updated_at: Option<i64>,

    // Pool pair composition (LP only) — token + quote symbol via market.
    token_symbol: Option<String>,
    quote_symbol: Option<String>,
}

pub struct VaultController {
    db: Arc<PostgresDatabase>,
}

impl VaultController {
    pub fn new(db: Arc<PostgresDatabase>) -> Self {
        Self { db }
    }

    pub async fn get_token_vaults(&self, token_id: &str) -> Result<TokenVaultsResponse> {
        let rows = measure_postgres!(
            "vault.get_token_vaults",
            sqlx::query_as::<_, VaultRow>(
                r#"
                SELECT
                    a.vault_id,
                    a.bps,
                    m.name,
                    m.vault_type,
                    m.active,

                    burn.quote_spent      AS burn_quote_spent,
                    burn.tokens_burned    AS burn_tokens_burned,
                    burn.burn_count       AS burn_count,
                    burn.updated_at       AS burn_updated_at,

                    lp.quote_injected     AS lp_quote_injected,
                    lp.token_injected     AS lp_token_injected,
                    lp.lp_burned          AS lp_lp_burned,
                    lp.inject_count       AS lp_inject_count,
                    lp.updated_at         AS lp_updated_at,

                    cf.current_balance    AS cf_current_balance,
                    cf.total_deposited    AS cf_total_deposited,
                    cf.total_claimed      AS cf_total_claimed,
                    cf.deposit_count      AS cf_deposit_count,
                    cf.claim_count        AS cf_claim_count,
                    cf.updated_at         AS cf_updated_at,

                    g.current_state       AS gift_current_state,
                    g.current_balance     AS gift_current_balance,
                    g.total_deposited     AS gift_total_deposited,
                    g.total_claimed       AS gift_total_claimed,
                    g.total_expired       AS gift_total_expired,
                    g.platform            AS gift_platform,
                    g.platform_id         AS gift_platform_id,
                    g.receiver            AS gift_receiver,
                    g.buyback_quote_spent AS gift_buyback_quote_spent,
                    g.buyback_tokens      AS gift_buyback_tokens,
                    g.updated_at          AS gift_updated_at,

                    tk.symbol             AS token_symbol,
                    qt.symbol             AS quote_symbol
                FROM v2_creator_fee_allocation a
                JOIN v2_vault_metadata m
                    ON m.vault_id = a.vault_id
                LEFT JOIN v2_burn_vault_stats burn
                    ON m.vault_type = 'BURN' AND burn.token_id = a.token_id
                LEFT JOIN v2_lp_vault_stats lp
                    ON m.vault_type = 'LP' AND lp.token_id = a.token_id
                LEFT JOIN v2_creator_fee_vault_stats cf
                    ON m.vault_type = 'CREATOR_FEE' AND cf.token_id = a.token_id
                LEFT JOIN v2_gift_vault_stats g
                    ON m.vault_type = 'GIFT' AND g.token_id = a.token_id
                LEFT JOIN token tk
                    ON tk.token_id = a.token_id
                LEFT JOIN market mk
                    ON mk.token_id = a.token_id
                LEFT JOIN quote_token qt
                    ON qt.quote_id = mk.quote_id
                WHERE a.token_id = $1
                ORDER BY a.bps DESC
                "#,
            )
            .bind(token_id)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token vaults: {}", err))?;

        let vaults = rows.into_iter().map(map_row).collect();

        Ok(TokenVaultsResponse {
            token_id: token_id.to_string(),
            vaults,
        })
    }
}

fn map_row(row: VaultRow) -> VaultEntry {
    let (last_executed_at, stats) = match row.vault_type {
        VaultType::Burn => (
            row.burn_updated_at.unwrap_or(0),
            VaultStats::Burn(BurnStats {
                quote_spent: bd_to_string(row.burn_quote_spent),
                tokens_burned: bd_to_string(row.burn_tokens_burned),
                execution_count: row.burn_count.unwrap_or(0),
            }),
        ),
        VaultType::Lp => (
            row.lp_updated_at.unwrap_or(0),
            VaultStats::Lp(LpStats {
                quote_injected: bd_to_string(row.lp_quote_injected),
                token_injected: bd_to_string(row.lp_token_injected),
                lp_burned: bd_to_string(row.lp_lp_burned),
                pool_pair: format_pool_pair(
                    row.token_symbol.as_deref(),
                    row.quote_symbol.as_deref(),
                ),
                execution_count: row.lp_inject_count.unwrap_or(0),
            }),
        ),
        VaultType::CreatorFee => (
            row.cf_updated_at.unwrap_or(0),
            VaultStats::CreatorFee(CreatorFeeStats {
                current_balance: bd_to_string(row.cf_current_balance),
                total_deposited: bd_to_string(row.cf_total_deposited),
                total_claimed: bd_to_string(row.cf_total_claimed),
                deposit_count: row.cf_deposit_count.unwrap_or(0),
                claim_count: row.cf_claim_count.unwrap_or(0),
            }),
        ),
        VaultType::Gift => (
            row.gift_updated_at.unwrap_or(0),
            VaultStats::Gift(GiftStats {
                current_state: row
                    .gift_current_state
                    .unwrap_or_else(|| "Accumulating".to_string()),
                current_balance: bd_to_string(row.gift_current_balance),
                total_deposited: bd_to_string(row.gift_total_deposited),
                total_claimed: bd_to_string(row.gift_total_claimed),
                total_expired: bd_to_string(row.gift_total_expired),
                platform: row.gift_platform,
                platform_id: row.gift_platform_id,
                receiver: row.gift_receiver,
                buyback_quote_spent: bd_to_string(row.gift_buyback_quote_spent),
                buyback_tokens: bd_to_string(row.gift_buyback_tokens),
            }),
        ),
        VaultType::Custom => (0, VaultStats::Custom(EmptyStats {})),
    };

    VaultEntry {
        vault_id: row.vault_id,
        bps: row.bps.max(0) as u32,
        name: row.name,
        active: row.active,
        last_executed_at,
        stats,
    }
}

fn bd_to_string(value: Option<BigDecimal>) -> String {
    value.map(|v| v.to_string()).unwrap_or_else(|| "0".to_string())
}

fn format_pool_pair(token_symbol: Option<&str>, quote_symbol: Option<&str>) -> String {
    match (token_symbol, quote_symbol) {
        (Some(t), Some(q)) => format!("{}/{}", t, q),
        (Some(t), None) => t.to_string(),
        _ => String::new(),
    }
}
