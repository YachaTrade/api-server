use std::sync::Arc;

use alloy::providers::{Provider, ProviderBuilder};
use bigdecimal::BigDecimal;
use tracing::error;

use crate::{
    config::{BONDING_CURVE, RPC_URL, WMON},
    controllers::terminal::TerminalController,
    db::postgres::PostgresDatabase,
    result::AppError,
    types::terminal::{
        Asset, AssetResponse, Block, Event, EventsResponse, LatestBlockResponse, Pair,
        PairResponse, Reserves,
    },
};

use crate::controllers::terminal::{SwapEventRow, MintEventRow, BurnEventRow};

/// Truncate BigDecimal to maximum 50 decimal places and convert to string
fn to_truncated_string(value: &BigDecimal) -> String {
    // Normalize and round to 50 decimal places
    let normalized = value.normalized();
    let truncated = normalized.with_scale(50);
    truncated.normalized().to_plain_string()
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
        let controller = TerminalController::new(self.postgres.clone());
        let asset_row = controller.get_asset(token_id).await.map_err(|e| {
            error!("Failed to get asset: {}", e);
            AppError::NotFound(format!("Asset not found: {}", e))
        })?;

        // Total supply is always 1 billion (1,000,000,000)
        let total_supply = BigDecimal::from(1_000_000_000u64);

        // Remove 18 decimals from circulating_supply (divide by 10^18)
        let decimals_divisor = BigDecimal::from(1_000_000_000_000_000_000u64); // 10^18
        let circulating_supply = asset_row.total_supply / decimals_divisor;

        let asset = Asset {
            id: asset_row.token_id,
            name: asset_row.name,
            symbol: asset_row.symbol,
            decimals: 18,
            total_supply: Some(total_supply.normalized().to_plain_string()),
            circulating_supply: Some(circulating_supply.normalized().to_plain_string()),
            coin_gecko_id: Some("monad".to_string()),
        };

        Ok(AssetResponse { asset })
    }

    pub async fn get_pair(&self, token_id: &str) -> Result<PairResponse, AppError> {
        let controller = TerminalController::new(self.postgres.clone());
        let pair_row = controller.get_pair(token_id).await.map_err(|e| {
            error!("Failed to get pair: {}", e);
            AppError::NotFound(format!("Pair not found: {}", e))
        })?;

        // Get block number from transaction hash
        let block_number = controller
            .get_block_number_by_tx(&pair_row.transaction_hash)
            .await
            .map_err(|e| {
                error!("Failed to get block number: {}", e);
                AppError::InternalError(format!("Failed to get block number: {}", e))
            })?;

        // Order assets: compare token_id with WMON alphabetically
        // asset0 should be the one that comes first alphabetically
        let (asset0_id, asset1_id) = if token_id.to_lowercase() < WMON.to_lowercase() {
            (token_id.to_string(), WMON.to_string())
        } else {
            (WMON.to_string(), token_id.to_string())
        };

        // Use pool_id if available, otherwise use BONDING_CURVE
        let pair_id = pair_row
            .pool_id
            .unwrap_or_else(|| BONDING_CURVE.to_string());

        // Determine dex_key based on market_type
        let dex_key = match pair_row.market_type.as_str() {
            "DEX" => "capricorn",
            _ => "nadfun",
        };

        let pair = Pair {
            id: pair_id,
            dex_key: dex_key.to_string(),
            asset0_id,
            asset1_id,
            created_at_block_number: Some(block_number),
            created_at_block_timestamp: Some(pair_row.created_at as u64),
            created_at_txn_id: Some(pair_row.transaction_hash),
            creator: Some(pair_row.creator),
            fee_bps: Some(100), // 1% fee
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
        events.extend(swap_rows.into_iter().map(|row| Self::convert_swap_event(row)));

        // Process mint events (join events)
        events.extend(mint_rows.into_iter().map(|row| Self::convert_mint_event(row)));

        // Process burn events (exit events)
        events.extend(burn_rows.into_iter().map(|row| Self::convert_burn_event(row)));

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
        // Decimals divisor: 10^18
        let decimals_divisor = BigDecimal::from(1_000_000_000_000_000_000u64);

        // Decimalize amounts (amount / 10^18)
        let native_amount_decimalized = &row.native_amount / &decimals_divisor;
        let token_amount_decimalized = &row.token_amount / &decimals_divisor;
        let reserve_native_decimalized = row
            .reserve_native
            .as_ref()
            .map(|r| r / &decimals_divisor)
            .unwrap_or_default();
        let reserve_token_decimalized = row
            .reserve_token
            .as_ref()
            .map(|r| r / &decimals_divisor)
            .unwrap_or_default();

        // Determine token0/token1 by comparing WMON (native) and token_id alphabetically
        // token0 comes first alphabetically
        let is_native_token0 = WMON.to_lowercase() < row.token_id.to_lowercase();

        // Map amounts based on token order and is_buy
        let (asset0_in, asset1_in, asset0_out, asset1_out) =
            match (is_native_token0, row.is_buy) {
                // token0 = WMON (native), token1 = token, Buy: native in, token out
                (true, true) => (
                    Some(to_truncated_string(&native_amount_decimalized)),
                    None,
                    None,
                    Some(to_truncated_string(&token_amount_decimalized)),
                ),
                // token0 = WMON (native), token1 = token, Sell: token in, native out
                (true, false) => (
                    None,
                    Some(to_truncated_string(&token_amount_decimalized)),
                    Some(to_truncated_string(&native_amount_decimalized)),
                    None,
                ),
                // token0 = token, token1 = WMON (native), Buy: native in, token out
                (false, true) => (
                    None,
                    Some(to_truncated_string(&native_amount_decimalized)),
                    Some(to_truncated_string(&token_amount_decimalized)),
                    None,
                ),
                // token0 = token, token1 = WMON (native), Sell: token in, native out
                (false, false) => (
                    Some(to_truncated_string(&token_amount_decimalized)),
                    None,
                    None,
                    Some(to_truncated_string(&native_amount_decimalized)),
                ),
            };

        // Calculate reserves based on token order
        let (reserve_asset0, reserve_asset1) = match is_native_token0 {
            // token0 = native, token1 = token
            true => (
                to_truncated_string(&reserve_native_decimalized),
                to_truncated_string(&reserve_token_decimalized),
            ),
            // token0 = token, token1 = native
            false => (
                to_truncated_string(&reserve_token_decimalized),
                to_truncated_string(&reserve_native_decimalized),
            ),
        };

        // Calculate priceNative = amount(asset1) / amount(asset0)
        // This is the price of asset0 quoted in asset1
        let price_native =
            match is_native_token0 {
                // token0 = native, token1 = token
                // priceNative = amount(asset1) / amount(asset0) = token_amount / native_amount
                true => to_truncated_string(&(&token_amount_decimalized / &native_amount_decimalized)),
                // token0 = token, token1 = native
                // priceNative = amount(asset1) / amount(asset0) = native_amount / token_amount
                false => to_truncated_string(&(&native_amount_decimalized / &token_amount_decimalized)),
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
            pair_id: row.pool_id.unwrap_or_else(|| BONDING_CURVE.to_string()),
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
        let native_amount_decimalized = &row.native_amount / &decimals_divisor;
        let token_amount_decimalized = &row.token_amount / &decimals_divisor;
        let reserve_native_decimalized = &row.reserve_native / &decimals_divisor;
        let reserve_token_decimalized = &row.reserve_token / &decimals_divisor;

        // Determine token0/token1 by comparing WMON (native) and token_id alphabetically
        let is_native_token0 = WMON.to_lowercase() < row.token_id.to_lowercase();

        // Map amounts based on token order
        // Mint (Join): both assets are added to the pool
        let (amount0, amount1) = match is_native_token0 {
            // token0 = native, token1 = token
            true => (
                to_truncated_string(&native_amount_decimalized),
                to_truncated_string(&token_amount_decimalized),
            ),
            // token0 = token, token1 = native
            false => (
                to_truncated_string(&token_amount_decimalized),
                to_truncated_string(&native_amount_decimalized),
            ),
        };

        // Calculate reserves based on token order
        let (reserve_asset0, reserve_asset1) = match is_native_token0 {
            // token0 = native, token1 = token
            true => (
                to_truncated_string(&reserve_native_decimalized),
                to_truncated_string(&reserve_token_decimalized),
            ),
            // token0 = token, token1 = native
            false => (
                to_truncated_string(&reserve_token_decimalized),
                to_truncated_string(&reserve_native_decimalized),
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
        let native_amount_decimalized = &row.native_amount / &decimals_divisor;
        let token_amount_decimalized = &row.token_amount / &decimals_divisor;
        let reserve_native_decimalized = &row.reserve_native / &decimals_divisor;
        let reserve_token_decimalized = &row.reserve_token / &decimals_divisor;

        // Determine token0/token1 by comparing WMON (native) and token_id alphabetically
        let is_native_token0 = WMON.to_lowercase() < row.token_id.to_lowercase();

        // Map amounts based on token order
        // Burn (Exit): both assets are removed from the pool
        let (amount0, amount1) = match is_native_token0 {
            // token0 = native, token1 = token
            true => (
                to_truncated_string(&native_amount_decimalized),
                to_truncated_string(&token_amount_decimalized),
            ),
            // token0 = token, token1 = native
            false => (
                to_truncated_string(&token_amount_decimalized),
                to_truncated_string(&native_amount_decimalized),
            ),
        };

        // Calculate reserves based on token order
        let (reserve_asset0, reserve_asset1) = match is_native_token0 {
            // token0 = native, token1 = token
            true => (
                to_truncated_string(&reserve_native_decimalized),
                to_truncated_string(&reserve_token_decimalized),
            ),
            // token0 = token, token1 = native
            false => (
                to_truncated_string(&reserve_token_decimalized),
                to_truncated_string(&reserve_native_decimalized),
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
