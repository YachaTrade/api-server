use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

#[derive(Debug)]
pub enum Identifier {
    Nickname(String),
    Address(String), // 이더리움 주소
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct CreateTokenResponse {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: Option<String>,
    pub created_at: i64,
    pub price: String,
    // pub total_supply: String,
    pub is_listing: bool,
    pub transaction_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct HoldTokenResponse {
    pub token_id: String,
    pub symbol: String,
    pub price: String,
    pub amount: String,
    pub image_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct AccountInfo {
    pub nickname: String,
    pub account_id: String,
    pub image_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct TokenInfoResponse {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
pub struct TokenInfo {
    pub token_id: String,
    pub name: String,
    pub symbol: String,
    pub image_uri: String,
    pub description: String,
    pub reply_count: String,
    pub price: String, //market.price
    pub reserve_token: String,
    pub created_at: i64,
    pub market_type: String,
    pub is_king: bool,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrderTokenRaw {
    pub token_id: String,
    pub account_id: String,
    pub nickname: String,
    pub account_image_uri: String,
    pub name: String,
    pub symbol: String,
    pub token_image_uri: String,
    pub description: Option<String>,
    pub reply_count: String,
    pub price: String,
    pub reserve_token: String,
    pub is_king: bool,
    pub market_type: String,
    pub created_at: i64,
    pub score: f64,
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrderToken {
    pub token_info: TokenInfo,

    pub account_info: AccountInfo,
}
impl From<OrderTokenRaw> for OrderToken {
    fn from(row: OrderTokenRaw) -> Self {
        OrderToken {
            token_info: TokenInfo {
                token_id: row.token_id,
                name: row.name,
                symbol: row.symbol,
                image_uri: row.token_image_uri,
                description: row.description.unwrap_or_default(),
                reply_count: row.reply_count,
                price: row.price,
                reserve_token: row.reserve_token,
                created_at: row.created_at,
                market_type: row.market_type,
                is_king: row.is_king,
                score: row.score,
            },
            account_info: AccountInfo {
                account_id: row.account_id,
                image_uri: row.account_image_uri,
                nickname: row.nickname,
            },
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchResponse {
    pub tokens: Vec<OrderToken>,
}

// #[derive(Debug, Clone, Serialize, Deserialize, FromRow, ToSchema)]
// pub struct MintPartyResponse {
//     pub account_info: AccountInfo,
//     pub mint_party_id: String,
//     pub name: String,
//     pub symbol: String,
//     pub description: String,
//     pub image_uri: String,
//     pub current_white_list_count: String,
//     pub allow_white_list_count: String,
//     pub funding_amount: String,
//     pub total_deposit_amount: String,
// }
// #[derive(FromRow)]
// pub struct MintPartyRaw {
//     // Account fields
//     pub account_id: String,
//     pub nickname: String,
//     pub account_image_uri: String,
//     // MintParty fields
//     pub mint_party_id: String,
//     pub name: String,
//     pub symbol: String,
//     pub description: Option<String>,
//     pub image_uri: String,
//     pub current_white_list_count: i16,
//     pub allow_white_list_count: i16,
//     pub funding_amount: BigDecimal,
//     pub total_deposit_amount: BigDecimal,
// }

// impl From<MintPartyRaw> for MintPartyResponse {
//     fn from(row: MintPartyRaw) -> Self {
//         Self {
//             account_info: AccountInfo {
//                 account_id: row.account_id,
//                 nickname: row.nickname,
//                 image_uri: row.account_image_uri,
//             },
//             mint_party_id: row.mint_party_id,
//             name: row.name,
//             symbol: row.symbol,
//             description: row.description.unwrap_or_default(),
//             image_uri: row.image_uri,
//             current_white_list_count: row.current_white_list_count.to_string(),
//             allow_white_list_count: row.allow_white_list_count.to_string(),
//             funding_amount: row.funding_amount.to_string(),
//             total_deposit_amount: row.total_deposit_amount.to_string(),
//         }
//     }
// }

// #[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
// pub struct MintPartyDepositListRaw {
//     pub account_id: String,
//     pub account_nickname: String,
//     pub account_image_uri: String,
//     pub comment: Option<String>,
//     pub mint_party_id: String,
//     pub mint_party_name: String,
//     pub mint_party_symbol: String,
//     pub mint_party_image_uri: String,
//     pub created_at: i64,
//     pub amount: BigDecimal,
//     pub is_white_list: bool,
//     pub transaction_hash: String,
// }

// impl From<MintPartyDepositListRaw> for MintPartyDepositList {
//     fn from(raw: MintPartyDepositListRaw) -> Self {
//         Self {
//             account_info: AccountInfo {
//                 account_id: raw.account_id,
//                 nickname: raw.account_nickname,
//                 image_uri: raw.account_image_uri,
//             },
//             comment: raw.comment,
//             mint_party_info: MintPartyInfo {
//                 mint_party_id: raw.mint_party_id,
//                 mint_party_name: raw.mint_party_name,
//                 mint_party_symbol: raw.mint_party_symbol,
//                 mint_party_image_uri: raw.mint_party_image_uri,
//             },
//             created_at: raw.created_at,
//             amount: raw.amount,
//             is_white_list: raw.is_white_list,
//             transaction_hash: raw.transaction_hash,
//         }
//     }
// }

// #[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
// pub struct MintPartyDepositList {
//     pub account_info: AccountInfo,
//     pub comment: Option<String>,
//     pub mint_party_info: MintPartyInfo,
//     pub created_at: i64,
//     pub amount: BigDecimal,
//     pub is_white_list: bool,
//     pub transaction_hash: String,
// }

// #[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
// pub struct MintPartyInfo {
//     pub mint_party_id: String,
//     pub mint_party_name: String,
//     pub mint_party_symbol: String,
//     pub mint_party_image_uri: String,
// }

// #[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type, ToSchema)]
// #[sqlx(type_name = "mint_party_claim_status")]
// pub enum MintPartyClaimStatus {
//     Pending,
//     Approved,
//     Closed,
//     Success,
// }

// #[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
// pub struct MintPartyBalanceRaw {
//     pub account_id: String,
//     pub account_nickname: String,
//     pub account_image_uri: String,
//     pub mint_party_id: String,
//     pub mint_party_name: String,
//     pub mint_party_symbol: String,
//     pub mint_party_image_uri: String,
//     pub amount: BigDecimal,
//     pub claim_status: MintPartyClaimStatus,
//     pub created_at: i64,
//     pub is_claimable: bool,
//     pub transaction_hash: String,
//     pub token_id: Option<String>,
// }

// impl From<MintPartyBalanceRaw> for MintPartyBalance {
//     fn from(raw: MintPartyBalanceRaw) -> Self {
//         Self {
//             account_info: AccountInfo {
//                 account_id: raw.account_id,
//                 nickname: raw.account_nickname,
//                 image_uri: raw.account_image_uri,
//             },
//             mint_party_info: MintPartyInfo {
//                 mint_party_id: raw.mint_party_id,
//                 mint_party_name: raw.mint_party_name,
//                 mint_party_symbol: raw.mint_party_symbol,
//                 mint_party_image_uri: raw.mint_party_image_uri,
//             },
//             amount: raw.amount,
//             claim_status: raw.claim_status,
//             created_at: raw.created_at,
//             is_claimable: raw.is_claimable,
//             transaction_hash: raw.transaction_hash,
//             token_id: raw.token_id,
//         }
//     }
// }

// #[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
// pub struct MintPartyBalance {
//     pub account_info: AccountInfo,
//     pub mint_party_info: MintPartyInfo,
//     pub amount: BigDecimal,
//     pub claim_status: MintPartyClaimStatus,
//     pub created_at: i64,
//     pub is_claimable: bool,
//     pub transaction_hash: String,
//     pub token_id: Option<String>,
// }
