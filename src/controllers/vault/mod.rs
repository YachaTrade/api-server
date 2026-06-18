use std::sync::Arc;

use anyhow::{Result, anyhow};
use sqlx::types::BigDecimal;

use crate::{
    db::postgres::PostgresDatabase,
    measure_postgres,
    types::common::info::QuoteInfo,
    types::vault::{
        BurnStats, CreatorFeeStats, DividendStats, DividendVaultTokenStat, EmptyStats, GiftStats,
        LpStats, TokenVaultsResponse, VaultEntry, VaultStats, VaultType,
    },
};

/// Display-only buffer subtracted from the chain-side gift expiry. The UI
/// countdown should hit 0 three days before on-chain EXPIRE becomes
/// claimable, giving users a window to act before fees redirect to
/// Buyback & Burn. Applied only to non-zero values — the `0` sentinel
/// (no SETUP yet or RECEIVER_SET cleared) passes through unchanged.
const GIFT_EXPIRY_DISPLAY_BUFFER_SECS: i64 = 3 * 24 * 60 * 60;

#[derive(Debug, sqlx::FromRow)]
struct VaultRow {
    vault_id: String,
    bps: i32,
    name: String,
    vault_type: VaultType,
    active: bool,

    // BURN — `v2_burn_vault_stats`
    burn_quote_spent: Option<BigDecimal>,
    // USD aggregates: COALESCE'd in SQL so they're never NULL — plain BigDecimal.
    burn_quote_spent_usd: BigDecimal,
    burn_tokens_burned: Option<BigDecimal>,
    burn_count: Option<i32>,
    burn_updated_at: Option<i64>,

    // LP — `v2_lp_vault_stats`
    lp_quote_injected: Option<BigDecimal>,
    lp_quote_injected_usd: BigDecimal,
    lp_token_injected: Option<BigDecimal>,
    lp_lp_burned: Option<BigDecimal>,
    lp_inject_count: Option<i32>,
    lp_updated_at: Option<i64>,

    // CREATOR_FEE — `v2_creator_fee_vault_stats`
    cf_current_balance: Option<BigDecimal>,
    // current_balance_usd: live `current_balance × latest_price / 10^decimals`,
    // COALESCE'd to 0 when no price row.
    cf_current_balance_usd: BigDecimal,
    cf_total_deposited: Option<BigDecimal>,
    cf_total_deposited_usd: BigDecimal,
    cf_total_claimed: Option<BigDecimal>,
    cf_total_claimed_usd: BigDecimal,
    cf_deposit_count: Option<i32>,
    cf_claim_count: Option<i32>,
    cf_updated_at: Option<i64>,
    cf_creator_id: Option<String>,

    // GIFT — `v2_gift_vault_stats`
    gift_current_state: Option<String>,
    gift_current_balance: Option<BigDecimal>,
    gift_current_balance_usd: BigDecimal,
    gift_total_deposited: Option<BigDecimal>,
    gift_total_deposited_usd: BigDecimal,
    gift_total_claimed: Option<BigDecimal>,
    gift_total_claimed_usd: BigDecimal,
    gift_total_expired: Option<BigDecimal>,
    gift_total_expired_usd: BigDecimal,
    gift_platform: Option<String>,
    gift_platform_id: Option<String>,
    gift_receiver: Option<String>,
    gift_buyback_quote_spent: Option<BigDecimal>,
    gift_buyback_quote_spent_usd: BigDecimal,
    gift_buyback_tokens: Option<BigDecimal>,
    gift_updated_at: Option<i64>,
    // GIFT verification window — both columns are NOT NULL with default 0
    // on v2_gift_vault_stats; they're Option<i64> here only because the
    // outer LEFT JOIN can produce NULL rows for non-GIFT vaults.
    gift_expires_at: Option<i64>,
    gift_receiver_set_at: Option<i64>,

    // Pool pair composition (LP only) — token + quote symbol via market.
    token_symbol: Option<String>,
    quote_symbol: Option<String>,

    // Per-(token, vault) distributed fee total — `v2_creator_fee_distribution_stats`.
    dist_distributed_quote: Option<BigDecimal>,
    dist_distributed_quote_usd: BigDecimal,
    // Token's market.quote_id — used to denote `total_quote_amount`.
    market_quote_id: Option<String>,
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

                    burn.quote_spent                  AS burn_quote_spent,
                    COALESCE(burn.quote_spent_usd, 0) AS burn_quote_spent_usd,
                    burn.tokens_burned                AS burn_tokens_burned,
                    burn.burn_count                   AS burn_count,
                    burn.updated_at                   AS burn_updated_at,

                    lp.quote_injected                  AS lp_quote_injected,
                    COALESCE(lp.quote_injected_usd, 0) AS lp_quote_injected_usd,
                    lp.token_injected                  AS lp_token_injected,
                    lp.lp_burned                       AS lp_lp_burned,
                    lp.inject_count                    AS lp_inject_count,
                    lp.updated_at                      AS lp_updated_at,

                    cf.current_balance                  AS cf_current_balance,
                    COALESCE(cf.current_balance, 0) * COALESCE(lp_price.price, 0)
                        / NULLIF(POWER(10, COALESCE(qt.decimals, 18))::NUMERIC, 0)
                                                        AS cf_current_balance_usd,
                    cf.total_deposited                  AS cf_total_deposited,
                    COALESCE(cf.total_deposited_usd, 0) AS cf_total_deposited_usd,
                    cf.total_claimed                    AS cf_total_claimed,
                    COALESCE(cf.total_claimed_usd, 0)   AS cf_total_claimed_usd,
                    cf.deposit_count                    AS cf_deposit_count,
                    cf.claim_count                      AS cf_claim_count,
                    cf.updated_at                       AS cf_updated_at,
                    tk.creator                          AS cf_creator_id,

                    g.current_state                          AS gift_current_state,
                    g.current_balance                        AS gift_current_balance,
                    COALESCE(g.current_balance, 0) * COALESCE(lp_price.price, 0)
                        / NULLIF(POWER(10, COALESCE(qt.decimals, 18))::NUMERIC, 0)
                                                             AS gift_current_balance_usd,
                    g.total_deposited                        AS gift_total_deposited,
                    COALESCE(g.total_deposited_usd, 0)       AS gift_total_deposited_usd,
                    g.total_claimed                          AS gift_total_claimed,
                    COALESCE(g.total_claimed_usd, 0)         AS gift_total_claimed_usd,
                    g.total_expired                          AS gift_total_expired,
                    COALESCE(g.total_expired_usd, 0)         AS gift_total_expired_usd,
                    g.platform                               AS gift_platform,
                    g.platform_id                            AS gift_platform_id,
                    g.receiver                               AS gift_receiver,
                    g.buyback_quote_spent                    AS gift_buyback_quote_spent,
                    COALESCE(g.buyback_quote_spent_usd, 0)   AS gift_buyback_quote_spent_usd,
                    g.buyback_tokens                         AS gift_buyback_tokens,
                    g.updated_at                             AS gift_updated_at,
                    g.expires_at                             AS gift_expires_at,
                    g.receiver_set_at                        AS gift_receiver_set_at,

                    tk.symbol AS token_symbol,
                    qt.symbol AS quote_symbol,

                    COALESCE(dist.distributed_quote, 0)     AS dist_distributed_quote,
                    COALESCE(dist.distributed_quote_usd, 0) AS dist_distributed_quote_usd,
                    mk.quote_id                             AS market_quote_id
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
                LEFT JOIN LATERAL (
                    SELECT p.price
                    FROM price p
                    WHERE p.quote_id = mk.quote_id
                    ORDER BY p.block_number DESC
                    LIMIT 1
                ) lp_price ON true
                LEFT JOIN v2_creator_fee_distribution_stats dist
                    ON dist.token_id = a.token_id AND dist.vault_id = a.vault_id
                WHERE a.token_id = $1
                ORDER BY a.bps DESC
                "#,
            )
            .bind(token_id)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch token vaults: {}", err))?;

        let total_quote_amount: BigDecimal = rows
            .iter()
            .filter_map(|r| r.dist_distributed_quote.clone())
            .fold(BigDecimal::from(0), |acc, x| acc + x);

        let total_quote_amount_usd: BigDecimal = rows
            .iter()
            .map(|r| r.dist_distributed_quote_usd.clone())
            .fold(BigDecimal::from(0), |acc, x| acc + x);

        let quote_id = rows
            .iter()
            .find_map(|r| r.market_quote_id.clone())
            .unwrap_or_default();

        // Dividend vault detail isn't in the per-vault stat tables; fetch it from
        // the dividend schema once (at most one DIVIDEND vault per token).
        let dividend = if rows.iter().any(|r| r.vault_type == VaultType::Dividend) {
            Some(self.fetch_dividend_vault_stats(token_id).await?)
        } else {
            None
        };

        let vaults: Vec<VaultEntry> = rows
            .into_iter()
            .map(|r| map_row(r, &dividend))
            .collect();

        Ok(TokenVaultsResponse {
            token_id: token_id.to_string(),
            quote_id,
            total_quote_amount: total_quote_amount.normalized().to_plain_string(),
            total_quote_amount_usd: total_quote_amount_usd.normalized().to_plain_string(),
            vaults,
        })
    }

    /// Dividend-vault detail from the dividend schema (per-token breakdown,
    /// recipients from the published distribution). Returns (last_executed_at,
    /// stats); the vault's `bps` / `name` come from the parent `VaultEntry`.
    async fn fetch_dividend_vault_stats(&self, token_id: &str) -> Result<(i64, DividendStats)> {
        #[derive(Debug, sqlx::FromRow)]
        struct StatRow {
            dividend_token: String,
            ratio: i32,
            min_balance: BigDecimal,
            amount: BigDecimal,
            deposited: BigDecimal,
            deposited_usd: BigDecimal,
            dt_name: String,
            dt_symbol: String,
            dt_decimals: i32,
            dt_image_uri: String,
        }

        let rows = measure_postgres!(
            "vault.fetch_dividend_breakdown",
            sqlx::query_as::<_, StatRow>(
                r#"
                SELECT
                    s.dividend_token,
                    s.ratio,
                    s.min_balance,
                    COALESCE(st.dividend_balance, 0) AS amount,
                    COALESCE(st.total_deposited, 0) + COALESCE(st.total_pending_deposited, 0) AS deposited,
                    COALESCE(st.total_deposited_usd, 0) + COALESCE(st.total_pending_deposited_usd, 0) AS deposited_usd,
                    COALESCE(wl.name, qt.name, tk.name, '') AS dt_name,
                    COALESCE(wl.symbol, qt.symbol, tk.symbol, '') AS dt_symbol,
                    COALESCE(wl.decimals, qt.decimals, 18) AS dt_decimals,
                    COALESCE(wl.image_uri, qt.image_uri, tk.image_uri, '') AS dt_image_uri
                FROM v2_dividend_setups s
                LEFT JOIN v2_dividend_vault_stats st
                    ON st.source_token = s.source_token AND st.dividend_token = s.dividend_token
                LEFT JOIN whitelist_token wl ON wl.token_id = s.dividend_token AND wl.enabled
                LEFT JOIN quote_token qt ON qt.quote_id = s.dividend_token
                LEFT JOIN token tk ON tk.token_id = s.dividend_token
                WHERE s.source_token = $1
                ORDER BY s.entry_index
                "#,
            )
            .bind(token_id)
            .fetch_all(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch dividend breakdown: {}", err))?;

        let mut allocated_volume = BigDecimal::from(0);
        let mut total_dividends_usd = BigDecimal::from(0);
        let min_balance = rows
            .first()
            .map(|r| r.min_balance.clone())
            .unwrap_or_else(|| BigDecimal::from(0));

        let dividend_tokens: Vec<DividendVaultTokenStat> = rows
            .into_iter()
            .map(|r| {
                allocated_volume = allocated_volume.clone() + r.deposited.clone();
                total_dividends_usd = total_dividends_usd.clone() + r.deposited_usd.clone();
                DividendVaultTokenStat {
                    dividend_token_info: QuoteInfo {
                        quote_id: r.dividend_token,
                        name: r.dt_name,
                        symbol: r.dt_symbol,
                        decimals: r.dt_decimals.max(0) as u32,
                        image_uri: r.dt_image_uri,
                    },
                    ratio_bps: r.ratio,
                    amount: r.amount.normalized().to_plain_string(),
                    amount_usd: r.deposited_usd.normalized().to_plain_string(),
                }
            })
            .collect();

        #[derive(Debug, sqlx::FromRow)]
        struct ScalarRow {
            recipient_count: i64,
            last_executed_at: i64,
        }

        // recipient_count = holders carrying a leaf in the current published merkle
        // root (dividend_distribution is latest-only, one row per (source, holder,
        // dividend)), counted DISTINCT across dividend tokens. This is the actual
        // distribution set — NOT balance-eligibility — so it matches the profile
        // view's claimable basis.
        let scalars = measure_postgres!(
            "vault.fetch_dividend_scalars",
            sqlx::query_as::<_, ScalarRow>(
                r#"
                SELECT
                    COALESCE((SELECT COUNT(DISTINCT holder) FROM dividend_distribution
                              WHERE source_token = $1), 0)::bigint AS recipient_count,
                    COALESCE((SELECT MAX(updated_at) FROM dividend_pair_state
                              WHERE source_token = $1), 0)::bigint AS last_executed_at
                "#,
            )
            .bind(token_id)
            .fetch_one(self.db.get_read_pool())
        )
        .map_err(|err| anyhow!("Failed to fetch dividend scalars: {}", err))?;

        Ok((
            scalars.last_executed_at,
            DividendStats {
                allocated_volume: allocated_volume.normalized().to_plain_string(),
                total_dividends_usd: total_dividends_usd.normalized().to_plain_string(),
                min_balance: min_balance.normalized().to_plain_string(),
                recipient_count: scalars.recipient_count,
                dividend_tokens,
            },
        ))
    }
}

fn map_row(row: VaultRow, dividend: &Option<(i64, DividendStats)>) -> VaultEntry {
    let (last_executed_at, stats) = match row.vault_type {
        VaultType::Burn => (
            row.burn_updated_at.unwrap_or(0),
            VaultStats::Burn(BurnStats {
                quote_spent: bd_to_string(row.burn_quote_spent),
                quote_spent_usd: row.burn_quote_spent_usd.normalized().to_plain_string(),
                tokens_burned: bd_to_string(row.burn_tokens_burned),
                execution_count: row.burn_count.unwrap_or(0),
            }),
        ),
        VaultType::Lp => (
            row.lp_updated_at.unwrap_or(0),
            VaultStats::Lp(LpStats {
                quote_injected: bd_to_string(row.lp_quote_injected),
                quote_injected_usd: row.lp_quote_injected_usd.normalized().to_plain_string(),
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
                creator_id: row.cf_creator_id.unwrap_or_default(),
                current_balance: bd_to_string(row.cf_current_balance),
                current_balance_usd: row.cf_current_balance_usd.normalized().to_plain_string(),
                total_deposited: bd_to_string(row.cf_total_deposited),
                total_deposited_usd: row.cf_total_deposited_usd.normalized().to_plain_string(),
                total_claimed: bd_to_string(row.cf_total_claimed),
                total_claimed_usd: row.cf_total_claimed_usd.normalized().to_plain_string(),
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
                current_balance_usd: row.gift_current_balance_usd.normalized().to_plain_string(),
                total_deposited: bd_to_string(row.gift_total_deposited),
                total_deposited_usd: row.gift_total_deposited_usd.normalized().to_plain_string(),
                total_claimed: bd_to_string(row.gift_total_claimed),
                total_claimed_usd: row.gift_total_claimed_usd.normalized().to_plain_string(),
                total_expired: bd_to_string(row.gift_total_expired),
                total_expired_usd: row.gift_total_expired_usd.normalized().to_plain_string(),
                platform: row.gift_platform,
                platform_id: row.gift_platform_id,
                receiver: row.gift_receiver,
                buyback_quote_spent: bd_to_string(row.gift_buyback_quote_spent),
                buyback_quote_spent_usd: row.gift_buyback_quote_spent_usd.normalized().to_plain_string(),
                buyback_tokens: bd_to_string(row.gift_buyback_tokens),
                expires_at: match row.gift_expires_at.unwrap_or(0) {
                    0 => 0,
                    raw => raw.saturating_sub(GIFT_EXPIRY_DISPLAY_BUFFER_SECS),
                },
                // DB stores 0 as the not-yet-set sentinel.
                receiver_set_at: row.gift_receiver_set_at.filter(|&v| v > 0),
            }),
        ),
        VaultType::Custom => (0, VaultStats::Custom(EmptyStats {})),
        VaultType::Dividend => match dividend.clone() {
            Some((last, stats)) => (last, VaultStats::Dividend(stats)),
            None => (
                0,
                VaultStats::Dividend(DividendStats {
                    allocated_volume: "0".to_string(),
                    total_dividends_usd: "0".to_string(),
                    min_balance: "0".to_string(),
                    recipient_count: 0,
                    dividend_tokens: vec![],
                }),
            ),
        },
    };

    VaultEntry {
        vault_id: row.vault_id,
        bps: row.bps.max(0) as u32,
        name: row.name,
        active: row.active,
        quote_amount: bd_to_string(row.dist_distributed_quote),
        quote_amount_usd: row.dist_distributed_quote_usd.normalized().to_plain_string(),
        last_executed_at,
        stats,
    }
}

fn bd_to_string(value: Option<BigDecimal>) -> String {
    value
        .map(|v| v.normalized().to_plain_string())
        .unwrap_or_else(|| "0".to_string())
}

fn format_pool_pair(token_symbol: Option<&str>, quote_symbol: Option<&str>) -> String {
    match (token_symbol, quote_symbol) {
        (Some(t), Some(q)) => format!("{}/{}", t, q),
        (Some(t), None) => t.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    const TOKEN: &str = "0x000000000000000000000000000000000000Bb01";
    const QUOTE: &str = "0x3bd359C1119dA7Da1D913D1C4D2B7c461115433A"; // MON (seeded)
    const DIV2: &str = "0x000000000000000000000000000000000000Cc01";
    const HOLDER: &str = "0x000000000000000000000000000000000000Aa01";
    const HOLDER2: &str = "0x000000000000000000000000000000000000Aa02";

    fn ctrl(pool: PgPool) -> VaultController {
        VaultController::new(Arc::new(crate::db::postgres::PostgresDatabase {
            write_pool: pool.clone(),
            read_pool: pool,
        }))
    }

    // DIVIDEND vault stats: per-token breakdown + allocated volume + USD total +
    // distribution recipient count (DISTINCT holders) + last executed.
    #[sqlx::test(migrations = "./migrations-test")]
    async fn dividend_vault_stats(pool: PgPool) {
        sqlx::query(r#"INSERT INTO v2_dividend_setups (source_token,dividend_token,ratio,min_balance,entry_index,transaction_hash,block_number,created_at,log_index,tx_index) VALUES ($1,$2,2500,10,0,'0xs1',1,1,0,0)"#)
            .bind(TOKEN).bind(QUOTE).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO v2_dividend_setups (source_token,dividend_token,ratio,min_balance,entry_index,transaction_hash,block_number,created_at,log_index,tx_index) VALUES ($1,$2,7500,10,1,'0xs1',1,1,0,1)"#)
            .bind(TOKEN).bind(DIV2).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO v2_dividend_vault_stats (source_token,dividend_token,total_deposited,total_deposited_usd,total_pending_deposited,total_pending_deposited_usd,dividend_balance,updated_at) VALUES ($1,$2,400,800,100,200,1000,50)"#)
            .bind(TOKEN).bind(QUOTE).execute(&pool).await.unwrap();
        // Distribution leaves: HOLDER carries a leaf for both dividend tokens
        // (must still count as ONE recipient), HOLDER2 for one → DISTINCT = 2.
        sqlx::query(r#"INSERT INTO dividend_distribution (source_token,holder,dividend_token,merkle_root,amount,proof,status,created_at) VALUES ($1,$2,$3,'0xroot',100,ARRAY['0xa'],'AWAITING',10)"#)
            .bind(TOKEN).bind(HOLDER).bind(QUOTE).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO dividend_distribution (source_token,holder,dividend_token,merkle_root,amount,proof,status,created_at) VALUES ($1,$2,$3,'0xroot',50,ARRAY['0xb'],'AWAITING',10)"#)
            .bind(TOKEN).bind(HOLDER).bind(DIV2).execute(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO dividend_distribution (source_token,holder,dividend_token,merkle_root,amount,proof,status,created_at) VALUES ($1,$2,$3,'0xroot',30,ARRAY['0xc'],'AWAITING',10)"#)
            .bind(TOKEN).bind(HOLDER2).bind(QUOTE).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO dividend_pair_state (source_token,dividend_token,last_allocated_balance,last_snapshot_block,updated_at) VALUES ($1,$2,0,5,50)")
            .bind(TOKEN).bind(QUOTE).execute(&pool).await.unwrap();

        let (last_executed, stats) = ctrl(pool).fetch_dividend_vault_stats(TOKEN).await.unwrap();

        assert_eq!(last_executed, 50);
        assert_eq!(stats.allocated_volume, "500"); // (400+100) + (0+0)
        assert_eq!(stats.total_dividends_usd, "1000"); // (800+200) + 0
        assert_eq!(stats.min_balance, "10");
        assert_eq!(stats.recipient_count, 2); // DISTINCT holders in distribution (HOLDER counted once)
        assert_eq!(stats.dividend_tokens.len(), 2);
    }
}
