//! Bot signer loaded from `.env`.
//!
//! The only secret the bot holds; see plan §9 secret redaction rules.
//! The returned `PrivateKeySigner` has a manual `Debug` that masks the key,
//! so logging it is safe — but never print `signer.to_bytes()` or similar.

use std::str::FromStr;

use alloy::signers::local::PrivateKeySigner;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SignerError {
    /// The key was valid hex but not a valid secp256k1 scalar (vanishingly
    /// rare — value 0 or >= curve order). Caught at load so we never
    /// send tx with a dead key.
    #[error("BOT_PRIVATE_KEY is not a valid secp256k1 scalar")]
    InvalidScalar,
}

/// Build a signer from the hex-encoded private key string. `Config::validate`
/// already guarantees the string parses to 32 bytes of hex — this call only
/// reveals the semantic failure (invalid scalar).
pub fn load(private_key_hex: &str) -> Result<PrivateKeySigner, SignerError> {
    PrivateKeySigner::from_str(private_key_hex).map_err(|_| SignerError::InvalidScalar)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_known_test_vector() {
        // PK = 0x…01 → address 0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf
        // A widely documented Ethereum test vector (it's the first valid
        // secp256k1 scalar). Stronger than a nonzero assertion: pins that
        // alloy's derivation is the standard keccak256(pubkey)[12..] path.
        let pk = "0x0000000000000000000000000000000000000000000000000000000000000001";
        let signer = load(pk).expect("valid scalar");
        let expected: alloy::primitives::Address = "0x7E5F4552091A69125d5DfCb7b8C2659029395Bdf"
            .parse()
            .unwrap();
        assert_eq!(signer.address(), expected);
    }

    #[test]
    fn load_is_deterministic() {
        let pk = "0x2222222222222222222222222222222222222222222222222222222222222222";
        let a = load(pk).unwrap().address();
        let b = load(pk).unwrap().address();
        assert_eq!(a, b, "same private key must derive the same address");
    }

    #[test]
    fn loads_valid_key_without_0x_prefix() {
        let pk = "1111111111111111111111111111111111111111111111111111111111111111";
        assert!(load(pk).is_ok());
    }

    #[test]
    fn rejects_invalid_scalar_zero() {
        // Scalar 0 is not a valid secp256k1 private key.
        let pk = "0x0000000000000000000000000000000000000000000000000000000000000000";
        assert!(matches!(load(pk), Err(SignerError::InvalidScalar)));
    }
}
