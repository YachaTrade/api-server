use std::sync::Arc;

use alloy::providers::{Provider, ProviderBuilder};
use bigdecimal::BigDecimal;
use tracing::error;

use crate::{
    config::RPC_URL,
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

const DEX_KEY: &str = "nadswap";

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
        // quote assets). RpcMetaSource caches results for 1h and
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
        // instead of a hard-coded figure, and works for quote/external assets
        // too. circulatingSupply can't be derived on-chain
        // (locked balances need interpretation), so it stays nadfun-only
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
                        Some("ethereum".to_string()),
                    )
                }
                None => (None, None, None),
            };
        let total_supply = onchain_total.or(db_total);
        // circulating을 못 구하는 자산(quote/external)은 totalSupply로 대신 채운다.
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
        // emitted from /events, which for DEX markets is the pool address,
        // not the token address.
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
            id: pair_row.pool_id.unwrap_or_else(|| pool_id.to_string()),
            dex_key: DEX_KEY.to_string(),
            asset0_id,
            asset1_id,
            created_at_block_number: block_number,
            created_at_block_timestamp: Some(pair_row.created_at as u64),
            created_at_txn_id: Some(pair_row.transaction_hash),
            creator: Some(pair_row.creator),
            // MarketInfo/fee_config was removed. Do not publish a guessed total
            // fee: NadSwap's LP fee alone does not describe creator/protocol fees.
            fee_bps: None,
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
        events.extend(swap_rows.into_iter().map(Self::convert_swap_event));

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

    fn convert_swap_event(row: SwapEventRow) -> Event {
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
            pair_id: row.pool_id.unwrap_or_else(|| row.token_id.clone()),
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

    const QUOTE: &str = "0xBe3fa50514D9617ce645a02B34F595541AF02b6b";
    const POOL: &str = "0x00000000000000000000000000000000000000aa";
    const TOKEN: &str = "0xff00000000000000000000000000000000000000";

    fn e18(value: u64) -> BigDecimal {
        BigDecimal::from(value) * BigDecimal::from(1_000_000_000_000_000_000u64)
    }

    #[test]
    fn order_assets_sorts_by_lowercase_address() {
        let (asset0, asset1, quote_is_asset0) = order_assets(TOKEN, QUOTE);
        assert_eq!(asset0, QUOTE);
        assert_eq!(asset1, TOKEN);
        assert!(quote_is_asset0);
    }

    #[test]
    fn dex_swap_uses_pool_and_quote_order() {
        let event = TerminalService::convert_swap_event(SwapEventRow {
            account_id: "0xmaker".to_string(),
            token_id: TOKEN.to_string(),
            is_buy: true,
            quote_amount: e18(1),
            token_amount: e18(2),
            reserve_quote: Some(e18(100)),
            reserve_token: Some(e18(200)),
            created_at: 123,
            transaction_hash: "0xswap".to_string(),
            block_number: 10,
            tx_index: Some(0),
            log_index: 0,
            pool_id: Some(POOL.to_string()),
            price: e18(1),
            quote_id: QUOTE.to_string(),
        });

        match event {
            Event::Swap {
                asset0_in,
                asset1_out,
                pair_id,
                reserves,
                ..
            } => {
                assert_eq!(asset0_in.as_deref(), Some("1"));
                assert_eq!(asset1_out.as_deref(), Some("2"));
                assert_eq!(pair_id, POOL);
                assert_eq!(reserves.asset0, "100");
                assert_eq!(reserves.asset1, "200");
            }
            _ => panic!("expected swap event"),
        }
    }

    #[test]
    fn dex_join_and_exit_keep_pool_and_quote_order() {
        let mint = TerminalService::convert_mint_event(MintEventRow {
            token_id: TOKEN.to_string(),
            account_id: "0xmaker".to_string(),
            market_id: POOL.to_string(),
            quote_amount: e18(1),
            token_amount: e18(2),
            reserve_quote: e18(100),
            reserve_token: e18(200),
            created_at: 123,
            transaction_hash: "0xmint".to_string(),
            block_number: 10,
            tx_index: 0,
            log_index: 0,
            quote_id: QUOTE.to_string(),
        });
        match mint {
            Event::Join {
                amount0,
                amount1,
                pair_id,
                ..
            } => {
                assert_eq!(amount0, "1");
                assert_eq!(amount1, "2");
                assert_eq!(pair_id, POOL);
            }
            _ => panic!("expected join event"),
        }

        let burn = TerminalService::convert_burn_event(BurnEventRow {
            token_id: TOKEN.to_string(),
            account_id: "0xmaker".to_string(),
            market_id: POOL.to_string(),
            quote_amount: e18(1),
            token_amount: e18(2),
            reserve_quote: e18(100),
            reserve_token: e18(200),
            created_at: 123,
            transaction_hash: "0xburn".to_string(),
            block_number: 10,
            tx_index: 0,
            log_index: 0,
            quote_id: QUOTE.to_string(),
        });
        match burn {
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
            _ => panic!("expected exit event"),
        }
    }
}
