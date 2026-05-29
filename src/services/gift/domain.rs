//! Shared domain types across the gift service modules (parser, validator,
//! executor).

use alloy::primitives::Address;

/// A tweet that has passed text-level parsing. Not yet validated against
/// on-chain state or operator policy — that's the validator's job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedGift {
    pub tweet_id: String,
    pub author_username: String,
    pub token: Address,
    pub receiver: Address,
}
