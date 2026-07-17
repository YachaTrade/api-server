use std::sync::Arc;

use alloy::providers::{Provider, ProviderBuilder};
use bigdecimal::BigDecimal;
use tracing::error;

use crate::{
    config::{RPC_URL, V1_BONDING_CURVE, V2_BONDING_CURVE},
    controllers::terminal::TerminalController,
    db::postgres::PostgresDatabase,
    result::AppError,
    types::terminal::{
        Asset, AssetResponse, Block, Event, EventsResponse, LatestBlockResponse, Pair,
        PairResponse, Reserves,
    },
};

use crate::controllers::terminal::{BurnEventRow, MintEventRow, SwapEventRow};
use crate::services::pricing::{TokenMetaSource, meta::RpcMetaSource};

/// Truncate BigDecimal to maximum 50 decimal places and convert to string
fn to_truncated_string(value: &BigDecimal) -> String {
    // Normalize and round to 50 decimal places
    let normalized = value.normalized();
    let truncated = normalized.with_scale(50);
    truncated.normalized().to_plain_string()
}

/// LP fee on the NadSwap (NadFunPair) AMM — fixed 25 BPS (0.25%).
/// Source: nadfun-contract-v2/src/dex/NadFunPair.sol `LP_FEE_RATE = 25`.
const NADSWAP_LP_FEE_BPS: u32 = 25;

/// V1 fixed trading fee, 1% = 100 BPS.
const V1_FEE_BPS: u32 = 100;

/// GeckoTerminal `dexKey` (trading venue identifier) for a market_type.
fn dex_key_for(market_type: &str) -> &'static str {
    match market_type {
        "DEX" => "capricorn",
        "V2_CURVE" => "nadfun-v2",
        "V2_DEX" => "nadswap",
        // "CURVE" and any unknown value fall back to the V1 bonding-curve key.
        _ => "nadfun",
    }
}

/// `pairId` for a market_type. DEX markets use the pool address; curve markets
/// use the version's bonding-curve contract. Falls back to the bonding curve
/// when a DEX market somehow has no pool id.
fn pair_id_for(
    market_type: &str,
    pool_id: Option<&str>,
    v1_bonding_curve: &str,
    v2_bonding_curve: &str,
) -> String {
    match market_type {
        "DEX" => pool_id.unwrap_or(v1_bonding_curve).to_string(),
        "V2_DEX" => pool_id.unwrap_or(v2_bonding_curve).to_string(),
        "V2_CURVE" => v2_bonding_curve.to_string(),
        // "CURVE" and any unknown value map to the V1 bonding curve.
        _ => v1_bonding_curve.to_string(),
    }
}

/// Total trading fee in BPS for a market_type. V1 is fixed at 100. V2 sums the
/// applicable `fee_config` rates; returns `None` when fee_config is missing so
/// callers can omit `feeBps` rather than report a wrong 0%.
fn fee_bps_for(
    market_type: &str,
    creator_fee_rate: Option<i16>,
    curve_protocol_fee_rate: Option<i16>,
    dex_protocol_fee_rate: Option<i16>,
) -> Option<u32> {
    let nonneg = |r: i16| r.max(0) as u32;
    match market_type {
        "V2_CURVE" => Some(nonneg(creator_fee_rate?) + nonneg(curve_protocol_fee_rate?)),
        "V2_DEX" => {
            Some(NADSWAP_LP_FEE_BPS + nonneg(creator_fee_rate?) + nonneg(dex_protocol_fee_rate?))
        }
        // "CURVE", "DEX", and any unknown value use the fixed V1 fee.
        _ => Some(V1_FEE_BPS),
    }
}

/// Order (token, quote) into (asset0, asset1) by ascending lowercase address,
/// matching GeckoTerminal's alphabetical pairing. Returns
/// `(asset0, asset1, is_quote_token0)`.
fn order_assets(token_id: &str, quote_id: &str) -> (String, String, bool) {
    if quote_id.to_lowercase() < token_id.to_lowercase() {
        (quote_id.to_string(), token_id.to_string(), true)
    } else {
        (token_id.to_string(), quote_id.to_string(), false)
    }
}

pub struct TerminalService {
    postgres: Arc<PostgresDatabase>,
}

impl TerminalService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    pub async fn get_latest_block(&self) -> Result<LatestBlockResponse, AppError> {
        // Get the latest indexed block from balance_history
        let controller = TerminalController::new(self.postgres.clone());
        let latest_indexed_block = controller.get_latest_indexed_block().await.map_err(|e| {
            error!("Failed to get latest indexed block: {}", e);
            AppError::InternalError(format!("Failed to get latest indexed block: {}", e))
        })?;

        // Get the timestamp for this block from the provider
        let rpc_url: url::Url = RPC_URL
            .parse()
            .map_err(|e| AppError::InternalError(format!("Invalid RPC_URL: {}", e)))?;

        let provider = ProviderBuilder::new().connect_http(rpc_url);

        let block = provider
            .get_block_by_number(latest_indexed_block.into())
            .await
            .map_err(|e| {
                error!("Failed to get block: {}", e);
                AppError::InternalError(format!("Failed to get block: {}", e))
            })?;

        let block_timestamp = match block {
            Some(b) => b.header.timestamp,
            None => {
                error!("Block not found, using current timestamp");
                chrono::Utc::now().timestamp() as u64
            }
        };

        let response = LatestBlockResponse {
            block: Block {
                block_number: latest_indexed_block,
                block_timestamp,
            },
        };

        Ok(response)
    }

    pub async fn get_asset(&self, token_id: &str) -> Result<AssetResponse, AppError> {
        // name/symbol/decimals are served from the chain itself (the source of
        // truth for ERC-20 metadata — DB copies can be missing or NULL for
        // quote/whitelist assets). RpcMetaSource caches results for 1h and
        // caches failures too, so this public (rate-limit-exempt) endpoint
        // can't be used to amplify outbound RPC traffic.
        //
        // The DB row is still consulted for two things the chain call doesn't
        // give us: nadfun supply figures, and metadata resilience when the RPC
        // is down (an asset we index must not 404 just because the node blips).
        let controller = TerminalController::new(self.postgres.clone());
        let db_row = controller.get_asset(token_id).await.ok();
        let rpc_meta = RpcMetaSource::new().token_meta(token_id).await;

        let (name, symbol, decimals) = match (&rpc_meta, &db_row) {
            (Some(meta), _) => (meta.name.clone(), meta.symbol.clone(), meta.decimals as u8),
            (None, Some(row)) => (row.name.clone(), row.symbol.clone(), row.decimals as u8),
            (None, None) => {
                error!("Failed to get asset (rpc miss + db miss): {}", token_id);
                return Err(AppError::NotFound(format!("Asset not found: {}", token_id)));
            }
        };

        // totalSupply: on-chain `IERC20.totalSupply()` first (10-min cache,
        // failures cached), human units = raw ÷ 10^decimals — reflects burns
        // instead of a hard-coded figure, and works for quote/whitelist/
        // external assets too. circulatingSupply can't be derived on-chain
        // (locked/vault balances need interpretation), so it stays nadfun-only
        // from the DB row, which also serves as the totalSupply fallback when
        // the RPC is down.
        let onchain_total = match RpcMetaSource::total_supply(token_id).await {
            Some(raw) => raw.parse::<BigDecimal>().ok().map(|raw_supply| {
                let divisor = BigDecimal::new(1.into(), -(i64::from(decimals))); // 10^decimals
                (raw_supply / divisor).normalized().to_plain_string()
            }),
            None => None,
        };

        let (db_total, db_circulating, coin_gecko_id) =
            match db_row.and_then(|row| row.total_supply) {
                Some(raw_supply) => {
                    let total = BigDecimal::from(1_000_000_000u64);
                    let decimals_divisor = BigDecimal::from(1_000_000_000_000_000_000u64); // 10^18
                    (
                        Some(total.normalized().to_plain_string()),
                        Some(
                            (raw_supply / decimals_divisor)
                                .normalized()
                                .to_plain_string(),
                        ),
                        Some("monad".to_string()),
                    )
                }
                None => (None, None, None),
            };
        let total_supply = onchain_total.or(db_total);
        // circulating을 못 구하는 자산(quote/whitelist/외부)은 totalSupply로 대신 채운다.
        let circulating_supply = db_circulating.or_else(|| total_supply.clone());

        let asset = Asset {
            id: token_id.to_string(),
            name,
            symbol,
            decimals,
            total_supply,
            circulating_supply,
            coin_gecko_id,
        };

        Ok(AssetResponse { asset })
    }

    pub async fn get_pair(&self, pool_id: &str) -> Result<PairResponse, AppError> {
        // Query by pool_id — the `id` GeckoTerminal sends back is the pairId we
        // emitted from /events, which for DEX markets is the pool address
        // (`pair_id_for`), NOT the token address.
        let controller = TerminalController::new(self.postgres.clone());
        let pair_row = controller.get_pair_by_pool_id(pool_id).await.map_err(|e| {
            error!("Failed to get pair: {}", e);
            AppError::NotFound(format!("Pair not found: {}", e))
        })?;

        // Creation block from the token's creation tx. Optional: tokens created
        // without an initial buy have no balance_history row, so this is None
        // rather than an error — `createdAtBlockNumber` is then omitted.
        let block_number = controller
            .get_block_number_by_tx(&pair_row.transaction_hash)
            .await
            .map_err(|e| {
                error!("Failed to get block number: {}", e);
                AppError::InternalError(format!("Failed to get block number: {}", e))
            })?;

        // Order assets by quote_id (replaces the WMON-hardcoded ordering).
        let (asset0_id, asset1_id, _) = order_assets(&pair_row.token_id, &pair_row.quote_id);

        let pair = Pair {
            id: pair_id_for(
                &pair_row.market_type,
                pair_row.pool_id.as_deref(),
                &V1_BONDING_CURVE,
                &V2_BONDING_CURVE,
            ),
            dex_key: dex_key_for(&pair_row.market_type).to_string(),
            asset0_id,
            asset1_id,
            created_at_block_number: block_number,
            created_at_block_timestamp: Some(pair_row.created_at as u64),
            created_at_txn_id: Some(pair_row.transaction_hash),
            creator: Some(pair_row.creator),
            fee_bps: fee_bps_for(
                &pair_row.market_type,
                pair_row.creator_fee_rate,
                pair_row.curve_protocol_fee_rate,
                pair_row.dex_protocol_fee_rate,
            ),
        };

        Ok(PairResponse { pair })
    }

    pub async fn get_events(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> Result<EventsResponse, AppError> {
        let controller = TerminalController::new(self.postgres.clone());

        // Fetch all three event types in parallel
        let (swap_result, mint_result, burn_result) = tokio::join!(
            controller.get_events(from_block, to_block),
            controller.get_mint_events(from_block, to_block),
            controller.get_burn_events(from_block, to_block)
        );

        let swap_rows = swap_result.map_err(|e| {
            error!("Failed to get swap events: {}", e);
            AppError::InternalError(format!("Failed to get swap events: {}", e))
        })?;

        let mint_rows = mint_result.map_err(|e| {
            error!("Failed to get mint events: {}", e);
            AppError::InternalError(format!("Failed to get mint events: {}", e))
        })?;

        let burn_rows = burn_result.map_err(|e| {
            error!("Failed to get burn events: {}", e);
            AppError::InternalError(format!("Failed to get burn events: {}", e))
        })?;

        // Convert all events to unified Event enum
        let mut events: Vec<Event> = Vec::new();

        // Process swap events
        events.extend(
            swap_rows
                .into_iter()
                .map(|r| Self::convert_swap_event(r, &V1_BONDING_CURVE, &V2_BONDING_CURVE)),
        );

        // Process mint events (join events)
        events.extend(mint_rows.into_iter().map(Self::convert_mint_event));

        // Process burn events (exit events)
        events.extend(burn_rows.into_iter().map(Self::convert_burn_event));

        // Sort events by block_number, tx_index, and log_index
        events.sort_by(|a, b| {
            (a.block_number(), a.transaction_index(), a.log_index()).cmp(&(
                b.block_number(),
                b.transaction_index(),
                b.log_index(),
            ))
        });

        Ok(EventsResponse { events })
    }

    fn convert_swap_event(
        row: SwapEventRow,
        v1_bonding_curve: &str,
        v2_bonding_curve: &str,
    ) -> Event {
        let decimals_divisor = BigDecimal::from(1_000_000_000_000_000_000u64);

        let quote_amount_decimalized = &row.quote_amount / &decimals_divisor;
        let token_amount_decimalized = &row.token_amount / &decimals_divisor;
        let reserve_quote_decimalized = row
            .reserve_quote
            .as_ref()
            .map(|r| r / &decimals_divisor)
            .unwrap_or_default();
        let reserve_token_decimalized = row
            .reserve_token
            .as_ref()
            .map(|r| r / &decimals_divisor)
            .unwrap_or_default();

        // quote vs token ordering (replaces the old WMON-hardcoded comparison).
        let (_, _, is_quote_token0) = order_assets(&row.token_id, &row.quote_id);

        let (asset0_in, asset1_in, asset0_out, asset1_out) = match (is_quote_token0, row.is_buy) {
            (true, true) => (
                Some(to_truncated_string(&quote_amount_decimalized)),
                None,
                None,
                Some(to_truncated_string(&token_amount_decimalized)),
            ),
            (true, false) => (
                None,
                Some(to_truncated_string(&token_amount_decimalized)),
                Some(to_truncated_string(&quote_amount_decimalized)),
                None,
            ),
            (false, true) => (
                None,
                Some(to_truncated_string(&quote_amount_decimalized)),
                Some(to_truncated_string(&token_amount_decimalized)),
                None,
            ),
            (false, false) => (
                Some(to_truncated_string(&token_amount_decimalized)),
                None,
                None,
                Some(to_truncated_string(&quote_amount_decimalized)),
            ),
        };

        let (reserve_asset0, reserve_asset1) = if is_quote_token0 {
            (
                to_truncated_string(&reserve_quote_decimalized),
                to_truncated_string(&reserve_token_decimalized),
            )
        } else {
            (
                to_truncated_string(&reserve_token_decimalized),
                to_truncated_string(&reserve_quote_decimalized),
            )
        };

        let price_native = if is_quote_token0 {
            to_truncated_string(&(&token_amount_decimalized / &quote_amount_decimalized))
        } else {
            to_truncated_string(&(&quote_amount_decimalized / &token_amount_decimalized))
        };

        Event::Swap {
            block: Block {
                block_number: row.block_number as u64,
                block_timestamp: row.created_at as u64,
            },
            txn_id: row.transaction_hash,
            txn_index: row.tx_index.unwrap_or(0) as u32,
            event_index: row.log_index as u32,
            maker: row.account_id,
            pair_id: pair_id_for(
                &row.market_type,
                row.pool_id.as_deref(),
                v1_bonding_curve,
                v2_bonding_curve,
            ),
            asset0_in,
            asset1_in,
            asset0_out,
            asset1_out,
            price_native,
            reserves: Reserves {
                asset0: reserve_asset0,
                asset1: reserve_asset1,
            },
        }
    }

    fn convert_mint_event(row: MintEventRow) -> Event {
        // Decimals divisor: 10^18
        let decimals_divisor = BigDecimal::from(1_000_000_000_000_000_000u64);

        // Decimalize amounts
        let quote_amount_decimalized = &row.quote_amount / &decimals_divisor;
        let token_amount_decimalized = &row.token_amount / &decimals_divisor;
        let reserve_quote_decimalized = &row.reserve_quote / &decimals_divisor;
        let reserve_token_decimalized = &row.reserve_token / &decimals_divisor;

        let (_, _, is_quote_token0) = order_assets(&row.token_id, &row.quote_id);

        // Map amounts based on token order
        // Mint (Join): both assets are added to the pool
        let (amount0, amount1) = match is_quote_token0 {
            // token0 = quote, token1 = token
            true => (
                to_truncated_string(&quote_amount_decimalized),
                to_truncated_string(&token_amount_decimalized),
            ),
            // token0 = token, token1 = quote
            false => (
                to_truncated_string(&token_amount_decimalized),
                to_truncated_string(&quote_amount_decimalized),
            ),
        };

        // Calculate reserves based on token order
        let (reserve_asset0, reserve_asset1) = match is_quote_token0 {
            // token0 = quote, token1 = token
            true => (
                to_truncated_string(&reserve_quote_decimalized),
                to_truncated_string(&reserve_token_decimalized),
            ),
            // token0 = token, token1 = quote
            false => (
                to_truncated_string(&reserve_token_decimalized),
                to_truncated_string(&reserve_quote_decimalized),
            ),
        };

        Event::Join {
            block: Block {
                block_number: row.block_number as u64,
                block_timestamp: row.created_at as u64,
            },
            txn_id: row.transaction_hash,
            txn_index: row.tx_index as u32,
            event_index: row.log_index as u32,
            maker: row.account_id,
            pair_id: row.market_id,
            amount0,
            amount1,
            reserves: Reserves {
                asset0: reserve_asset0,
                asset1: reserve_asset1,
            },
        }
    }

    fn convert_burn_event(row: BurnEventRow) -> Event {
        // Decimals divisor: 10^18
        let decimals_divisor = BigDecimal::from(1_000_000_000_000_000_000u64);

        // Decimalize amounts
        let quote_amount_decimalized = &row.quote_amount / &decimals_divisor;
        let token_amount_decimalized = &row.token_amount / &decimals_divisor;
        let reserve_quote_decimalized = &row.reserve_quote / &decimals_divisor;
        let reserve_token_decimalized = &row.reserve_token / &decimals_divisor;

        let (_, _, is_quote_token0) = order_assets(&row.token_id, &row.quote_id);

        // Map amounts based on token order
        // Burn (Exit): both assets are removed from the pool
        let (amount0, amount1) = match is_quote_token0 {
            // token0 = quote, token1 = token
            true => (
                to_truncated_string(&quote_amount_decimalized),
                to_truncated_string(&token_amount_decimalized),
            ),
            // token0 = token, token1 = quote
            false => (
                to_truncated_string(&token_amount_decimalized),
                to_truncated_string(&quote_amount_decimalized),
            ),
        };

        // Calculate reserves based on token order
        let (reserve_asset0, reserve_asset1) = match is_quote_token0 {
            // token0 = quote, token1 = token
            true => (
                to_truncated_string(&reserve_quote_decimalized),
                to_truncated_string(&reserve_token_decimalized),
            ),
            // token0 = token, token1 = quote
            false => (
                to_truncated_string(&reserve_token_decimalized),
                to_truncated_string(&reserve_quote_decimalized),
            ),
        };

        Event::Exit {
            block: Block {
                block_number: row.block_number as u64,
                block_timestamp: row.created_at as u64,
            },
            txn_id: row.transaction_hash,
            txn_index: row.tx_index as u32,
            event_index: row.log_index as u32,
            maker: row.account_id,
            pair_id: row.market_id,
            amount0,
            amount1,
            reserves: Reserves {
                asset0: reserve_asset0,
                asset1: reserve_asset1,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controllers::terminal::{BurnEventRow, MintEventRow, SwapEventRow};
    use bigdecimal::BigDecimal;

    const WMON_ADDR: &str = "0x3bd359C1119dA7Da1D913D1C4D2B7c461115433A";
    const LVMON_ADDR: &str = "0xBe3fa50514D9617ce645a02B34F595541AF02b6b";
    const V1_BC: &str = "0x0000000000000000000000000000000000000001";
    const V2_BC: &str = "0x0000000000000000000000000000000000000002";
    const POOL: &str = "0x00000000000000000000000000000000000000aa";
    // Token whose lowercase address is greater than both quotes above.
    const TOKEN_HI: &str = "0xff00000000000000000000000000000000000000";

    #[test]
    fn dex_key_maps_all_market_types() {
        assert_eq!(dex_key_for("CURVE"), "nadfun");
        assert_eq!(dex_key_for("DEX"), "capricorn");
        assert_eq!(dex_key_for("V2_CURVE"), "nadfun-v2");
        assert_eq!(dex_key_for("V2_DEX"), "nadswap");
        assert_eq!(dex_key_for("UNKNOWN"), "nadfun");
    }

    #[test]
    fn pair_id_routes_by_market_type() {
        assert_eq!(pair_id_for("CURVE", None, V1_BC, V2_BC), V1_BC);
        assert_eq!(pair_id_for("V2_CURVE", None, V1_BC, V2_BC), V2_BC);
        assert_eq!(pair_id_for("DEX", Some(POOL), V1_BC, V2_BC), POOL);
        assert_eq!(pair_id_for("V2_DEX", Some(POOL), V1_BC, V2_BC), POOL);
        // DEX with missing pool falls back to the version's bonding curve.
        assert_eq!(pair_id_for("V2_DEX", None, V1_BC, V2_BC), V2_BC);
        assert_eq!(pair_id_for("DEX", None, V1_BC, V2_BC), V1_BC);
    }

    #[test]
    fn fee_bps_v1_is_fixed_100() {
        assert_eq!(fee_bps_for("CURVE", None, None, None), Some(100));
        assert_eq!(fee_bps_for("DEX", Some(500), Some(50), Some(30)), Some(100));
    }

    #[test]
    fn fee_bps_v2_curve_is_creator_plus_curve() {
        assert_eq!(
            fee_bps_for("V2_CURVE", Some(500), Some(50), Some(30)),
            Some(550)
        );
    }

    #[test]
    fn fee_bps_v2_dex_is_lp_plus_creator_plus_dex() {
        // 25 (LP) + 500 (creator) + 30 (dex) = 555
        assert_eq!(
            fee_bps_for("V2_DEX", Some(500), Some(50), Some(30)),
            Some(555)
        );
    }

    #[test]
    fn fee_bps_v2_missing_fee_config_is_none() {
        assert_eq!(fee_bps_for("V2_CURVE", None, None, None), None);
        assert_eq!(fee_bps_for("V2_DEX", None, None, None), None);
        assert_eq!(fee_bps_for("V2_DEX", Some(500), Some(50), None), None);
    }

    #[test]
    fn order_assets_sorts_by_lowercase_address() {
        // quote (WMON 0x3b...) < token (0xff...) => quote is asset0
        let (a0, a1, quote0) = order_assets(TOKEN_HI, WMON_ADDR);
        assert_eq!(a0, WMON_ADDR);
        assert_eq!(a1, TOKEN_HI);
        assert!(quote0);

        // token (0x00..01) < quote (LVMON 0xbe..) => token is asset0
        let low_token = "0x0000000000000000000000000000000000000abc";
        let (b0, b1, quote0b) = order_assets(low_token, LVMON_ADDR);
        assert_eq!(b0, low_token);
        assert_eq!(b1, LVMON_ADDR);
        assert!(!quote0b);
    }

    fn e18(n: u64) -> BigDecimal {
        BigDecimal::from(n) * BigDecimal::from(1_000_000_000_000_000_000u64)
    }

    fn swap_row(market_type: &str, quote_id: &str, is_buy: bool) -> SwapEventRow {
        SwapEventRow {
            account_id: "0xmaker".to_string(),
            token_id: TOKEN_HI.to_string(),
            is_buy,
            quote_amount: e18(1),
            token_amount: e18(2),
            reserve_quote: Some(e18(100)),
            reserve_token: Some(e18(200)),
            created_at: 123,
            transaction_hash: "0xs".to_string(),
            block_number: 10,
            tx_index: Some(0),
            log_index: 0,
            pool_id: Some(POOL.to_string()),
            price: e18(1),
            quote_id: quote_id.to_string(),
            market_type: market_type.to_string(),
        }
    }

    #[test]
    fn swap_v2_dex_lvmon_quote_buy_maps_quote_as_asset0() {
        // TOKEN_HI (0xff..) > LVMON (0xbe..) => quote is asset0.
        let ev =
            TerminalService::convert_swap_event(swap_row("V2_DEX", LVMON_ADDR, true), V1_BC, V2_BC);
        match ev {
            Event::Swap {
                asset0_in,
                asset1_in,
                asset0_out,
                asset1_out,
                price_native,
                reserves,
                pair_id,
                ..
            } => {
                assert_eq!(asset0_in, Some("1".to_string())); // quote in
                assert_eq!(asset1_out, Some("2".to_string())); // token out
                assert_eq!(asset1_in, None);
                assert_eq!(asset0_out, None);
                assert_eq!(reserves.asset0, "100"); // reserve_quote
                assert_eq!(reserves.asset1, "200"); // reserve_token
                assert_eq!(price_native, "2"); // token/quote
                assert_eq!(pair_id, POOL); // V2_DEX -> pool
            }
            _ => panic!("expected swap"),
        }
    }

    #[test]
    fn swap_pair_id_uses_per_event_market_type() {
        // A graduated token's curve-era swap must reference the bonding curve,
        // not the current pool.
        let ev = TerminalService::convert_swap_event(
            swap_row("V2_CURVE", LVMON_ADDR, true),
            V1_BC,
            V2_BC,
        );
        match ev {
            Event::Swap { pair_id, .. } => assert_eq!(pair_id, V2_BC),
            _ => panic!("expected swap"),
        }
    }

    #[test]
    fn swap_v1_wmon_quote_unchanged() {
        // Regression: WMON quote keeps the legacy asset ordering/pricing.
        let ev =
            TerminalService::convert_swap_event(swap_row("CURVE", WMON_ADDR, true), V1_BC, V2_BC);
        match ev {
            Event::Swap {
                asset0_in,
                asset1_out,
                pair_id,
                ..
            } => {
                assert_eq!(asset0_in, Some("1".to_string()));
                assert_eq!(asset1_out, Some("2".to_string()));
                assert_eq!(pair_id, V1_BC);
            }
            _ => panic!("expected swap"),
        }
    }

    #[test]
    fn mint_v2_dex_lvmon_quote_orders_by_quote() {
        let row = MintEventRow {
            token_id: TOKEN_HI.to_string(),
            account_id: "0xmaker".to_string(),
            market_id: POOL.to_string(),
            quote_amount: e18(1),
            token_amount: e18(2),
            reserve_quote: e18(100),
            reserve_token: e18(200),
            created_at: 123,
            transaction_hash: "0xm".to_string(),
            block_number: 10,
            tx_index: 0,
            log_index: 0,
            quote_id: LVMON_ADDR.to_string(),
        };
        match TerminalService::convert_mint_event(row) {
            Event::Join {
                amount0,
                amount1,
                reserves,
                pair_id,
                ..
            } => {
                assert_eq!(amount0, "1"); // quote side
                assert_eq!(amount1, "2"); // token side
                assert_eq!(reserves.asset0, "100");
                assert_eq!(reserves.asset1, "200");
                assert_eq!(pair_id, POOL); // market_id preserved
            }
            _ => panic!("expected join"),
        }
    }

    #[test]
    fn burn_v2_dex_lvmon_quote_orders_by_quote() {
        let row = BurnEventRow {
            token_id: TOKEN_HI.to_string(),
            account_id: "0xmaker".to_string(),
            market_id: POOL.to_string(),
            quote_amount: e18(1),
            token_amount: e18(2),
            reserve_quote: e18(100),
            reserve_token: e18(200),
            created_at: 123,
            transaction_hash: "0xb".to_string(),
            block_number: 10,
            tx_index: 0,
            log_index: 0,
            quote_id: LVMON_ADDR.to_string(),
        };
        match TerminalService::convert_burn_event(row) {
            Event::Exit {
                amount0,
                amount1,
                pair_id,
                ..
            } => {
                assert_eq!(amount0, "1");
                assert_eq!(amount1, "2");
                assert_eq!(pair_id, POOL);
            }
            _ => panic!("expected exit"),
        }
    }
}
