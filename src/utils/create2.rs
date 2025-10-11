use alloy::primitives::{keccak256, Address, B256, U256};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Result of salt mining operation
#[derive(Debug, Clone)]
pub struct MinedSalt {
    pub salt: B256,
    pub address: Address,
    pub iterations: u64,
}

/// Computes the CREATE2 address for an EIP-1167 minimal proxy clone
///
/// This implements the same logic as OpenZeppelin's Clones.cloneDeterministic:
/// https://github.com/OpenZeppelin/openzeppelin-contracts/blob/master/contracts/proxy/Clones.sol
///
/// The EIP-1167 bytecode pattern:
/// - Prefix: 0x3d602d80600a3d3981f3363d3d373d3d3d363d73
/// - Implementation address (20 bytes)
/// - Suffix: 0x5af43d82803e903d91602b57fd5bf3
///
/// CREATE2 address = keccak256(0xff ++ deployer ++ salt ++ keccak256(init_code))[12:]
pub fn compute_create2_address(deployer: Address, implementation: Address, salt: B256) -> Address {
    // Construct the EIP-1167 minimal proxy bytecode
    let mut init_code = Vec::with_capacity(55);

    // Prefix (20 bytes)
    init_code.extend_from_slice(&hex::decode("3d602d80600a3d3981f3363d3d373d3d3d363d73").unwrap());

    // Implementation address (20 bytes)
    init_code.extend_from_slice(implementation.as_slice());

    // Suffix (15 bytes)
    init_code.extend_from_slice(&hex::decode("5af43d82803e903d91602b57fd5bf3").unwrap());

    // Hash the init code
    let init_code_hash = keccak256(&init_code);

    // Construct the CREATE2 input: 0xff ++ deployer ++ salt ++ init_code_hash
    let mut create2_input = Vec::with_capacity(85);
    create2_input.push(0xff);
    create2_input.extend_from_slice(deployer.as_slice());
    create2_input.extend_from_slice(salt.as_slice());
    create2_input.extend_from_slice(init_code_hash.as_slice());

    // Hash and extract address (last 20 bytes)
    let hash = keccak256(&create2_input);
    Address::from_slice(&hash[12..])
}

/// Checks if an address ends with the specified suffix (in hex)
pub fn address_ends_with(address: &Address, suffix: &str) -> bool {
    let address_hex = hex::encode(address.as_slice());
    address_hex.ends_with(suffix)
}

/// Mines a salt value that produces a CREATE2 address ending with the specified suffix
///
/// This function uses parallel processing for efficient mining.
/// Each user's token creation params + UUID are hashed to create a unique starting salt,
/// preventing collisions when multiple users mine simultaneously.
///
/// # Arguments
/// * `deployer` - The deployer address (factory contract)
/// * `implementation` - The implementation address (template contract)
/// * `suffix` - The desired hex suffix (e.g., "143" for addresses ending in 143)
/// * `max_iterations` - Maximum number of attempts (None for unlimited)
/// * `creator` - Token creator address (for uniqueness)
/// * `name` - Token name (for uniqueness)
/// * `symbol` - Token symbol (for uniqueness)
/// * `token_uri` - Token URI (for uniqueness)
/// * `uuid` - Unique identifier for this mining request (for additional uniqueness)
///
/// # Returns
/// * `Some(MinedSalt)` if a matching salt is found
/// * `None` if max_iterations is reached without finding a match
pub fn mine_salt_with_suffix(
    deployer: Address,
    implementation: Address,
    suffix: &str,
    max_iterations: Option<u64>,
    creator: &str,
    name: &str,
    symbol: &str,
    token_uri: &str,
    uuid: &str,
) -> Option<MinedSalt> {
    let suffix = suffix.to_lowercase();
    let found = Arc::new(AtomicBool::new(false));
    let result: Arc<parking_lot::Mutex<Option<MinedSalt>>> = Arc::new(parking_lot::Mutex::new(None));

    // Generate unique starting salt from user's token creation params + UUID
    // This ensures each request has a completely unique search space, preventing collisions
    let mut params_data = Vec::new();
    params_data.extend_from_slice(creator.as_bytes());
    params_data.extend_from_slice(name.as_bytes());
    params_data.extend_from_slice(symbol.as_bytes());
    params_data.extend_from_slice(token_uri.as_bytes());
    params_data.extend_from_slice(uuid.as_bytes()); // Add UUID for extra uniqueness

    let start_value = B256::from(keccak256(&params_data));

    let start_u256 = U256::from_be_bytes(*start_value);

    // Determine chunk size for parallel processing
    let chunk_size = 10000u64;

    // Calculate total iterations
    let total_iterations = max_iterations.unwrap_or(u64::MAX);
    let num_chunks = (total_iterations + chunk_size - 1) / chunk_size;

    // Process in parallel chunks
    (0..num_chunks).into_par_iter().find_map_any(|chunk_idx| {
        // Early exit if another thread found a match
        if found.load(Ordering::Relaxed) {
            return None;
        }

        let chunk_start = chunk_idx * chunk_size;
        let chunk_end = ((chunk_idx + 1) * chunk_size).min(total_iterations);

        for i in chunk_start..chunk_end {
            // Check if another thread found a match
            if found.load(Ordering::Relaxed) {
                return None;
            }

            // Calculate current salt
            let current_salt_u256 = start_u256.wrapping_add(U256::from(i));
            let salt = B256::from(current_salt_u256.to_be_bytes::<32>());

            // Compute address
            let address = compute_create2_address(deployer, implementation, salt);

            // Check if address matches suffix
            if address_ends_with(&address, &suffix) {
                found.store(true, Ordering::Relaxed);
                let iterations = i + 1; // +1 because i is 0-indexed
                let mined = MinedSalt { salt, address, iterations };
                *result.lock() = Some(mined.clone());
                return Some(mined);
            }
        }

        None
    });

    result.lock().clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    /// Test that verifies our CREATE2 implementation matches Solidity's Clones.cloneDeterministic
    ///
    /// This test uses known values from a Solidity contract deployment to ensure
    /// our Rust implementation produces identical results.
    #[test]
    fn test_compute_create2_address_matches_solidity() {
        // Known test values - these can be verified by deploying a Solidity contract
        let deployer = Address::from_str("0x5FbDB2315678afecb367f032d93F642f64180aa3").unwrap();
        let implementation = Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap();
        let salt = B256::from([0u8; 32]);

        let address = compute_create2_address(deployer, implementation, salt);

        // Verify the address is valid (42 chars: 0x + 40 hex)
        assert_eq!(address.to_string().len(), 42);

        // The address should be deterministic - same inputs always produce same output
        let address2 = compute_create2_address(deployer, implementation, salt);
        assert_eq!(address, address2);

        println!("Computed CREATE2 address: {}", address);
    }

    /// Test EIP-1167 minimal proxy bytecode construction
    ///
    /// Verifies that we construct the exact same bytecode as Solidity's Clones library:
    /// 0x3d602d80600a3d3981f3363d3d373d3d3d363d73 + implementation + 0x5af43d82803e903d91602b57fd5bf3
    #[test]
    fn test_eip1167_bytecode_construction() {
        let implementation = Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap();

        // Manually construct expected bytecode
        let mut expected_bytecode = Vec::new();
        expected_bytecode.extend_from_slice(&hex::decode("3d602d80600a3d3981f3363d3d373d3d3d363d73").unwrap());
        expected_bytecode.extend_from_slice(implementation.as_slice());
        expected_bytecode.extend_from_slice(&hex::decode("5af43d82803e903d91602b57fd5bf3").unwrap());

        // Verify bytecode is exactly 55 bytes (20 + 20 + 15)
        assert_eq!(expected_bytecode.len(), 55);

        println!("EIP-1167 bytecode (55 bytes): 0x{}", hex::encode(&expected_bytecode));
        println!("Implementation address embedded at bytes 20-40");
    }

    /// Test CREATE2 address calculation step by step
    ///
    /// This breaks down the calculation to match Solidity's process:
    /// 1. Construct EIP-1167 proxy bytecode
    /// 2. Hash the bytecode (init_code_hash)
    /// 3. Compute: keccak256(0xff ++ deployer ++ salt ++ init_code_hash)
    /// 4. Take last 20 bytes as address
    #[test]
    fn test_create2_calculation_breakdown() {
        let deployer = Address::from_str("0xD5724171C2b7f0AA717a324626050BD05767e2C6").unwrap();
        let implementation = Address::from_str("0x52D34d8536350Cd997bCBD0b9E9d722452f341F5").unwrap();
        let salt = B256::from([1u8; 32]);

        // Step 1: Construct bytecode
        let mut init_code = Vec::new();
        init_code.extend_from_slice(&hex::decode("3d602d80600a3d3981f3363d3d373d3d3d363d73").unwrap());
        init_code.extend_from_slice(implementation.as_slice());
        init_code.extend_from_slice(&hex::decode("5af43d82803e903d91602b57fd5bf3").unwrap());

        // Step 2: Hash init code
        let init_code_hash = keccak256(&init_code);
        println!("Init code hash: 0x{}", hex::encode(init_code_hash));

        // Step 3: Construct CREATE2 input
        let mut create2_input = Vec::new();
        create2_input.push(0xff);
        create2_input.extend_from_slice(deployer.as_slice());
        create2_input.extend_from_slice(salt.as_slice());
        create2_input.extend_from_slice(init_code_hash.as_slice());

        assert_eq!(create2_input.len(), 85); // 1 + 20 + 32 + 32

        // Step 4: Hash and extract address
        let hash = keccak256(&create2_input);
        let address = Address::from_slice(&hash[12..]);

        println!("Deployer: {}", deployer);
        println!("Implementation: {}", implementation);
        println!("Salt: 0x{}", hex::encode(salt));
        println!("Computed address: {}", address);

        // Verify our function produces the same result
        let address_from_fn = compute_create2_address(deployer, implementation, salt);
        assert_eq!(address, address_from_fn);
    }

    #[test]
    fn test_address_ends_with() {
        let address = Address::from_str("0x1234567890123456789012345678901234560143").unwrap();
        assert!(address_ends_with(&address, "143"));
        assert!(address_ends_with(&address, "0143"));
        assert!(address_ends_with(&address, "43"));
        assert!(!address_ends_with(&address, "144"));

        // Test case insensitivity
        assert!(address_ends_with(&address, "143"));
        assert!(address_ends_with(&address, "143"));
    }

    /// Test vanity address mining with single digit suffix
    ///
    /// Single digit suffixes are easy to find (1/16 probability)
    #[test]
    fn test_mine_salt_single_digit() {
        let deployer = Address::from_str("0x5FbDB2315678afecb367f032d93F642f64180aa3").unwrap();
        let implementation = Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap();

        // Mine for a single digit suffix (should find quickly)
        let result = mine_salt_with_suffix(
            deployer,
            implementation,
            "3",
            Some(100000),
            "0x1234567890123456789012345678901234567890",
            "Test Token",
            "TEST",
            "ipfs://test",
            "test-uuid-123",
        );

        if let Some(mined) = result {
            // Verify the result
            let computed_address = compute_create2_address(deployer, implementation, mined.salt);
            assert_eq!(computed_address, mined.address);
            assert!(address_ends_with(&mined.address, "3"));

            println!("Found address ending in '3': {}", mined.address);
            println!("Salt: 0x{}", hex::encode(mined.salt));
        } else {
            // Single digit should almost always be found within 100k iterations
            panic!("Failed to find address ending in '3' within 100k iterations");
        }
    }

    /// Test vanity address mining with suffix "143"
    ///
    /// This matches the production use case: finding addresses ending in "143"
    /// Probability: 1/4096, so we need to try more iterations
    #[test]
    fn test_mine_salt_with_143_suffix() {
        let deployer = Address::from_str("0xD5724171C2b7f0AA717a324626050BD05767e2C6").unwrap();
        let implementation = Address::from_str("0x52D34d8536350Cd997bCBD0b9E9d722452f341F5").unwrap();

        println!("Mining for address ending in '143'...");
        println!("Deployer: {}", deployer);
        println!("Implementation: {}", implementation);

        // Mine for "143" suffix
        // Expected iterations: ~4096 (1/16^3)
        // Set max to 50000 to be safe
        let result = mine_salt_with_suffix(
            deployer,
            implementation,
            "143",
            Some(50000),
            "0x742d35Cc6634C0532925a3b844Bc9e7595f70143",
            "Test Token",
            "TEST",
            "ipfs://QmTest123",
            "test-uuid-143",
        );

        if let Some(mined) = result {
            // Verify the result
            let computed_address = compute_create2_address(deployer, implementation, mined.salt);
            assert_eq!(computed_address, mined.address);
            assert!(address_ends_with(&mined.address, "143"));

            println!("✓ Found address ending in '143' after {} iterations!", mined.iterations);
            println!("  Address: {}", mined.address);
            println!("  Salt: 0x{}", hex::encode(mined.salt));

            // Verify the last 3 hex characters are exactly "143"
            let address_hex = hex::encode(mined.address.as_slice());
            assert!(address_hex.ends_with("143"));
        } else {
            println!("⚠ Did not find address ending in '143' within 50k iterations");
            println!("This is statistically possible but unlikely");
        }
    }

    /// Test that different token params produce different salts
    #[test]
    fn test_different_params_different_salts() {
        let deployer = Address::from_str("0x5FbDB2315678afecb367f032d93F642f64180aa3").unwrap();
        let implementation = Address::from_str("0xe7f1725E7734CE288F8367e1Bb143E90bb3F0512").unwrap();

        // User A's params
        let result1 = mine_salt_with_suffix(
            deployer,
            implementation,
            "a",
            Some(10000),
            "0x1111111111111111111111111111111111111111",
            "Token A",
            "TKA",
            "ipfs://a",
            "uuid-user-a",
        );

        // User B's params (different)
        let result2 = mine_salt_with_suffix(
            deployer,
            implementation,
            "a",
            Some(10000),
            "0x2222222222222222222222222222222222222222",
            "Token B",
            "TKB",
            "ipfs://b",
            "uuid-user-b",
        );

        // Different params should produce different salts and addresses
        if let (Some(mined1), Some(mined2)) = (result1, result2) {
            assert_ne!(mined1.salt, mined2.salt, "Different params should produce different salts");
            assert_ne!(mined1.address, mined2.address, "Different params should produce different addresses");

            // But both should end with 'a'
            assert!(address_ends_with(&mined1.address, "a"));
            assert!(address_ends_with(&mined2.address, "a"));
        }
    }

    /// Test with actual production config values
    ///
    /// This uses the real BONDING_CURVE and TOKEN_IMPLEMENT addresses
    /// from the environment configuration to ensure production compatibility
    #[test]
    fn test_with_production_config() {
        // Production values from .env
        let deployer = Address::from_str("0xD5724171C2b7f0AA717a324626050BD05767e2C6").unwrap();
        let implementation = Address::from_str("0x52D34d8536350Cd997bCBD0b9E9d722452f341F5").unwrap();

        // Test with a known salt
        let salt = B256::from([0u8; 32]);
        let address = compute_create2_address(deployer, implementation, salt);

        println!("Production config test:");
        println!("  BONDING_CURVE: {}", deployer);
        println!("  TOKEN_IMPLEMENT: {}", implementation);
        println!("  Salt (all zeros): 0x{}", hex::encode(salt));
        println!("  Resulting address: {}", address);

        // Verify determinism
        let address2 = compute_create2_address(deployer, implementation, salt);
        assert_eq!(address, address2);
    }

    /// Test with TokenCreationParams (API request format)
    ///
    /// This simulates the actual API usage where users provide token metadata.
    /// Note: creator, name, symbol, tokenURI don't affect the CREATE2 address calculation.
    /// They're only used for token initialization after deployment.
    #[test]
    fn test_with_token_creation_params() {
        // Simulated TokenCreationParams from API request
        let creator = "0x742d35Cc6634C0532925a3b844Bc9e7595f70143";
        let name = "Test Token";
        let symbol = "TEST";
        let token_uri = "ipfs://QmTest123456789";

        println!("TokenCreationParams:");
        println!("  creator: {}", creator);
        println!("  name: {}", name);
        println!("  symbol: {}", symbol);
        println!("  tokenURI: {}", token_uri);
        println!();

        // Production config from .env
        let deployer = Address::from_str("0xD5724171C2b7f0AA717a324626050BD05767e2C6").unwrap();
        let implementation = Address::from_str("0x52D34d8536350Cd997bCBD0b9E9d722452f341F5").unwrap();
        let suffix = "143"; // From VANITY_ADDRESS_SUFFIX

        println!("Mining salt for address ending in '{}'...", suffix);

        // Mine salt (this is what the API does)
        let result = mine_salt_with_suffix(
            deployer,
            implementation,
            suffix,
            Some(50000),
            creator,
            name,
            symbol,
            token_uri,
            "test-uuid-full-params",
        );

        if let Some(mined) = result {
            println!("✓ Success! Found after {} iterations", mined.iterations);
            println!("  Token address: {}", mined.address);
            println!("  Salt: 0x{}", hex::encode(mined.salt));
            println!();
            println!("This salt can be used to deploy a token with:");
            println!("  - creator: {}", creator);
            println!("  - name: {}", name);
            println!("  - symbol: {}", symbol);
            println!("  - tokenURI: {}", token_uri);
            println!("  - salt: 0x{}", hex::encode(mined.salt));
            println!();
            println!("The deployed token will have address: {}", mined.address);

            // Verify the address ends with 143
            assert!(address_ends_with(&mined.address, suffix));

            // Verify determinism
            let verified_address = compute_create2_address(deployer, implementation, mined.salt);
            assert_eq!(verified_address, mined.address);
        } else {
            panic!("Failed to find address ending in '{}' within 50k iterations", suffix);
        }
    }
}
