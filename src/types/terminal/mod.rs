use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

// Request/Response types based on gecko.md spec

#[derive(Debug, Serialize, ToSchema)]
pub struct Block {
    #[serde(rename = "blockNumber")]
    pub block_number: u64,
    #[serde(rename = "blockTimestamp")]
    pub block_timestamp: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LatestBlockResponse {
    pub block: Block,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Asset {
    pub id: String,
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "totalSupply")]
    pub total_supply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "circulatingSupply")]
    pub circulating_supply: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "coinGeckoId")]
    pub coin_gecko_id: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AssetResponse {
    pub asset: Asset,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct AssetQuery {
    pub id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Pair {
    pub id: String,
    #[serde(rename = "dexKey")]
    pub dex_key: String,
    #[serde(rename = "asset0Id")]
    pub asset0_id: String,
    #[serde(rename = "asset1Id")]
    pub asset1_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createdAtBlockNumber")]
    pub created_at_block_number: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createdAtBlockTimestamp")]
    pub created_at_block_timestamp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "createdAtTxnId")]
    pub created_at_txn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "feeBps")]
    pub fee_bps: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PairResponse {
    pub pair: Pair,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PairQuery {
    pub id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Reserves {
    pub asset0: String,
    pub asset1: String,
}

#[derive(Debug, Serialize, ToSchema)]
#[serde(tag = "eventType")]
pub enum Event {
    #[serde(rename = "swap")]
    Swap {
        block: Block,
        #[serde(rename = "txnId")]
        txn_id: String,
        #[serde(rename = "txnIndex")]
        txn_index: u32,
        #[serde(rename = "eventIndex")]
        event_index: u32,
        maker: String,
        #[serde(rename = "pairId")]
        pair_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "asset0In")]
        asset0_in: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "asset1In")]
        asset1_in: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "asset0Out")]
        asset0_out: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        #[serde(rename = "asset1Out")]
        asset1_out: Option<String>,
        #[serde(rename = "priceNative")]
        price_native: String,
        reserves: Reserves,
    },
    #[serde(rename = "join")]
    Join {
        block: Block,
        #[serde(rename = "txnId")]
        txn_id: String,
        #[serde(rename = "txnIndex")]
        txn_index: u32,
        #[serde(rename = "eventIndex")]
        event_index: u32,
        maker: String,
        #[serde(rename = "pairId")]
        pair_id: String,
        amount0: String,
        amount1: String,
        reserves: Reserves,
    },
    #[serde(rename = "exit")]
    Exit {
        block: Block,
        #[serde(rename = "txnId")]
        txn_id: String,
        #[serde(rename = "txnIndex")]
        txn_index: u32,
        #[serde(rename = "eventIndex")]
        event_index: u32,
        maker: String,
        #[serde(rename = "pairId")]
        pair_id: String,
        amount0: String,
        amount1: String,
        reserves: Reserves,
    },
}

impl Event {
    pub fn block_number(&self) -> u64 {
        match self {
            Event::Swap { block, .. } => block.block_number,
            Event::Join { block, .. } => block.block_number,
            Event::Exit { block, .. } => block.block_number,
        }
    }

    pub fn transaction_index(&self) -> u32 {
        match self {
            Event::Swap { txn_index, .. } => *txn_index,
            Event::Join { txn_index, .. } => *txn_index,
            Event::Exit { txn_index, .. } => *txn_index,
        }
    }

    pub fn log_index(&self) -> u32 {
        match self {
            Event::Swap { event_index, .. } => *event_index,
            Event::Join { event_index, .. } => *event_index,
            Event::Exit { event_index, .. } => *event_index,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EventsResponse {
    pub events: Vec<Event>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct EventsQuery {
    #[serde(rename = "fromBlock")]
    pub from_block: u64,
    #[serde(rename = "toBlock")]
    pub to_block: u64,
}

impl EventsQuery {
    const MAX_BLOCK_RANGE: u64 = 10000;

    pub fn validate(&self) -> Result<(), String> {
        if self.from_block > self.to_block {
            return Err("from_block must be <= to_block".to_string());
        }
        if self.to_block - self.from_block > Self::MAX_BLOCK_RANGE {
            return Err(format!("Block range must not exceed {}", Self::MAX_BLOCK_RANGE));
        }
        Ok(())
    }
}
