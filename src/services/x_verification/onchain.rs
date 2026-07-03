//! On-chain deploy check for the pre-deploy reservation (design §7.1-B).
//!
//! The security boundary: a reservation may only be created while the token is
//! NOT yet deployed (its salt is therefore still secret). We read `eth_getCode`
//! and treat ANY RPC failure as "cannot confirm undeployed" ⇒ reject the
//! reservation (fail-closed). Never optimistically assume "not deployed".

use alloy::primitives::Address;
use alloy::providers::{Provider, ProviderBuilder};

use crate::config::RPC_URL;
use crate::result::AppError;

/// Pure: an address already holds contract code ⇒ it is deployed.
pub fn code_indicates_deployed(code: &[u8]) -> bool {
    !code.is_empty()
}

/// Fetch `eth_getCode(token_id)` (latest block) and report whether the address
/// is already deployed. Fail-closed: any RPC/parse error ⇒ `ServiceUnavailable`
/// so the caller rejects the reservation (a chain-node outage must NOT be read
/// as "undeployed" — that would reopen C1).
pub async fn is_contract_deployed(token_id: &str) -> Result<bool, AppError> {
    let rpc_url: url::Url = RPC_URL
        .parse()
        .map_err(|e| AppError::InternalError(format!("Invalid RPC_URL: {e}")))?;
    let provider = ProviderBuilder::new().connect_http(rpc_url);
    let addr: Address = token_id
        .parse()
        .map_err(|_| AppError::BadRequest("invalid token_id".into()))?;
    let code = provider
        .get_code_at(addr)
        .await
        .map_err(|e| AppError::ServiceUnavailable(format!("onchain_check_unavailable: {e}")))?;
    Ok(code_indicates_deployed(&code))
}

#[cfg(test)]
mod tests {
    use super::code_indicates_deployed;

    #[test]
    fn empty_code_is_not_deployed() {
        assert!(!code_indicates_deployed(&[]));
    }

    #[test]
    fn nonempty_code_is_deployed() {
        // A tiny slice of real runtime bytecode prefix.
        assert!(code_indicates_deployed(&[0x60, 0x80, 0x60, 0x40, 0x52]));
    }
}
