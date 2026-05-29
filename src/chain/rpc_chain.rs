//! Multi-endpoint RPC failover chain for the consumer.
//!
//! Holds 1–3 alloy providers — `[main, sub1?, sub2?]` — each already bound
//! to the bot signer's wallet. All reads try the main endpoint first,
//! falling through to sub1 then sub2 on timeout or error. Writes are
//! re-submitted on every endpoint since the same signed raw tx is
//! idempotent (`AlreadyKnown` from a secondary node counts as success —
//! Monad nodes share a mempool).
//!
//! **Boot invariant (split-architecture §7.2):** every endpoint MUST
//! respond to `eth_chainId` with the configured `MONAD_CHAIN_ID`. A
//! mismatch on *any* endpoint is terminal — silent failover across
//! mismatched chains would read stale state (false `rejected`) or send
//! funds to the wrong network.

use std::time::Duration;

use alloy::network::{EthereumWallet, TransactionBuilder};
use alloy::primitives::{Address, B256, Bytes, TxHash, U256};
use alloy::providers::{DynProvider, Provider, ProviderBuilder};
use alloy::rpc::types::{TransactionReceipt, TransactionRequest};
use alloy::signers::local::PrivateKeySigner;
use alloy::sol_types::SolCall;
use thiserror::Error;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::chain::bindings::GiftVault;

/// Why an `RpcChain` boot or call failed.
#[derive(Debug, Error)]
pub enum RpcChainError {
    #[error("failed to construct RPC provider for {url}: {reason}")]
    Connect { url: String, reason: String },

    #[error(
        "chain id mismatch on endpoint {url}: expected {expected}, got {actual} — \
         terminal, refusing to accept writes against a wrong-chain RPC"
    )]
    ChainIdMismatch {
        url: String,
        expected: u64,
        actual: u64,
    },

    #[error("eth_chainId RPC call failed on {url}: {reason}")]
    ChainIdQuery { url: String, reason: String },

    /// Every endpoint in the chain failed or timed out for this call.
    /// Carries the last error from the primary so operators get one
    /// concrete line instead of N "also failed" logs.
    #[error("all RPC endpoints failed for {op}: {reason}")]
    AllEndpointsFailed { op: &'static str, reason: String },

    #[error("shutdown signaled during RpcChain operation")]
    Cancelled,
}

/// Handle to a single endpoint. `label` is the URL — used for logging so
/// operators see which node handled a given request.
#[derive(Clone)]
pub struct Endpoint {
    pub url: String,
    pub provider: DynProvider,
}

/// Failover chain of 1–3 alloy providers.
#[derive(Clone)]
pub struct RpcChain {
    endpoints: Vec<Endpoint>,
    signer_address: Address,
    read_timeout: Duration,
    send_timeout: Duration,
}

impl RpcChain {
    /// Connect to every configured endpoint and hard-check chain_id on
    /// each. Returns error on first mismatch so the operator sees a real
    /// URL, not a generic "boot failed".
    ///
    /// `signer` is cloned into each provider's wallet filler so `send_tx`
    /// on any endpoint produces an identically signed raw tx.
    pub async fn connect(
        urls: &[String],
        expected_chain_id: u64,
        signer: PrivateKeySigner,
        read_timeout: Duration,
        send_timeout: Duration,
        shutdown: &CancellationToken,
    ) -> Result<Self, RpcChainError> {
        if urls.is_empty() {
            return Err(RpcChainError::Connect {
                url: "<none>".to_string(),
                reason: "no RPC endpoints configured".to_string(),
            });
        }

        let signer_address = signer.address();

        let mut endpoints = Vec::with_capacity(urls.len());
        for url in urls {
            // Shutdown-aware: if a slow endpoint handshake stalls during
            // boot, honor SIGTERM rather than force the operator to `kill
            // -9`.
            let provider = tokio::select! {
                biased;
                _ = shutdown.cancelled() => return Err(RpcChainError::Cancelled),
                p = Self::build_provider(url, signer.clone()) => p?,
            };

            let actual = tokio::select! {
                biased;
                _ = shutdown.cancelled() => return Err(RpcChainError::Cancelled),
                v = provider.get_chain_id() => v.map_err(|e| RpcChainError::ChainIdQuery {
                    url: url.clone(),
                    reason: e.to_string(),
                })?,
            };

            if actual != expected_chain_id {
                error!(
                    url = %url,
                    expected = expected_chain_id,
                    actual,
                    "RPC chain-id mismatch; endpoint is pointed at the wrong network"
                );
                return Err(RpcChainError::ChainIdMismatch {
                    url: url.clone(),
                    expected: expected_chain_id,
                    actual,
                });
            }
            info!(url = %url, chain_id = actual, "RPC endpoint verified");

            endpoints.push(Endpoint {
                url: url.clone(),
                provider,
            });
        }

        Ok(Self {
            endpoints,
            signer_address,
            read_timeout,
            send_timeout,
        })
    }

    /// Test-only constructor that skips chain_id checks. Intended for
    /// anvil-backed unit tests.
    #[cfg(test)]
    pub(crate) fn for_testing(
        endpoints: Vec<Endpoint>,
        signer_address: Address,
        read_timeout: Duration,
        send_timeout: Duration,
    ) -> Self {
        Self {
            endpoints,
            signer_address,
            read_timeout,
            send_timeout,
        }
    }

    pub fn signer_address(&self) -> Address {
        self.signer_address
    }

    /// URLs of every configured endpoint. Used by boot logs + tests.
    #[allow(dead_code)]
    pub fn endpoint_urls(&self) -> Vec<&str> {
        self.endpoints.iter().map(|e| e.url.as_str()).collect()
    }

    /// Primary endpoint — kept for tests + ad-hoc reads that don't
    /// need failover. Production code should prefer `call_read` /
    /// `send_setreceiver` so endpoint health drives behavior.
    #[allow(dead_code)]
    pub fn primary(&self) -> &DynProvider {
        &self.endpoints[0].provider
    }

    async fn build_provider(
        url: &str,
        signer: PrivateKeySigner,
    ) -> Result<DynProvider, RpcChainError> {
        // `ProviderBuilder::connect` auto-detects transport from the URL
        // scheme: http/https → HTTP, ws/wss → WebSocket with alloy's
        // built-in reconnect. Keeps the branch off our side.
        let wallet = EthereumWallet::from(signer);
        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect(url)
            .await
            .map_err(|e| RpcChainError::Connect {
                url: url.to_string(),
                reason: e.to_string(),
            })?;
        Ok(DynProvider::new(provider))
    }

    /// Fetch `getGiftInfo(token)` via the fallback chain. Logs which
    /// endpoint answered so operators can correlate reads with node
    /// health.
    pub async fn get_gift_info(
        &self,
        vault: Address,
        token: Address,
    ) -> Result<GiftVault::GiftInfo, RpcChainError> {
        self.call_read("getGiftInfo", |ep| {
            let provider = ep.provider.clone();
            async move {
                let gv = GiftVault::new(vault, &provider);
                gv.getGiftInfo(token)
                    .call()
                    .await
                    .map_err(|e| e.to_string())
            }
        })
        .await
    }

    /// Fetch the pending nonce for the signer address via the fallback
    /// chain. Use pending tag so we don't race our own in-flight tx.
    pub async fn get_nonce_pending(&self) -> Result<u64, RpcChainError> {
        let addr = self.signer_address;
        self.call_read("get_transaction_count_pending", |ep| {
            let provider = ep.provider.clone();
            async move {
                provider
                    .get_transaction_count(addr)
                    .pending()
                    .await
                    .map_err(|e| e.to_string())
            }
        })
        .await
    }

    /// Estimate EIP-1559 fees via the fallback chain.
    pub async fn estimate_fees(&self) -> Result<Fees, RpcChainError> {
        self.call_read("estimate_eip1559_fees", |ep| {
            let provider = ep.provider.clone();
            async move {
                provider
                    .estimate_eip1559_fees()
                    .await
                    .map(|f| Fees {
                        max_fee_per_gas: f.max_fee_per_gas,
                        max_priority_fee_per_gas: f.max_priority_fee_per_gas,
                    })
                    .map_err(|e| e.to_string())
            }
        })
        .await
    }

    /// Fetch the receipt for a given tx hash — returns `None` if the tx
    /// is not yet mined on any endpoint. This is non-waiting: use it for
    /// reconciliation where we just want a point-in-time answer, not a
    /// confirmation wait. Currently unused (executor.rs uses the primary
    /// provider's fluent builder for the submit + receipt-wait combo),
    /// but kept in the public API for future phases.
    #[allow(dead_code)]
    pub async fn get_receipt(
        &self,
        hash: TxHash,
    ) -> Result<Option<TransactionReceipt>, RpcChainError> {
        self.call_read("get_transaction_receipt", |ep| {
            let provider = ep.provider.clone();
            async move {
                provider
                    .get_transaction_receipt(hash)
                    .await
                    .map_err(|e| e.to_string())
            }
        })
        .await
    }

    /// Probe `eth_getTransactionByHash`. Used for §6.1 reconciliation to
    /// decide whether a submitted tx is still pending, mined, or dropped
    /// from the mempool.
    ///
    /// Returns `TxProbe::Unknown` if the node hasn't seen the hash at
    /// all; that means the tx was dropped from the mempool (re-org, gas
    /// bump, or simple eviction) and should be resubmitted.
    pub async fn probe_transaction(&self, hash: TxHash) -> Result<TxProbe, RpcChainError> {
        let raw = self
            .call_read("get_transaction_by_hash", |ep| {
                let provider = ep.provider.clone();
                async move {
                    provider
                        .get_transaction_by_hash(hash)
                        .await
                        .map_err(|e| e.to_string())
                }
            })
            .await?;

        match raw {
            None => Ok(TxProbe::Unknown),
            Some(tx) => {
                // alloy's `Transaction` exposes block_number via the
                // `info()` helper (TransactionInfo).
                let block_number = tx.block_number;
                match block_number {
                    Some(n) => Ok(TxProbe::Mined { block_number: n }),
                    None => Ok(TxProbe::PendingInMempool),
                }
            }
        }
    }

    /// Submit a `setReceiver(token, receiver)` transaction with
    /// failover across every configured endpoint.
    ///
    /// Each endpoint's wallet filler holds a clone of the same signer,
    /// so the same nonce + same calldata produces an identical raw
    /// signed envelope on every endpoint. Monad nodes share the
    /// mempool — if endpoint A accepts the tx but the response never
    /// reaches us due to network glitch, endpoint B will report
    /// `AlreadyKnown`. We treat that as success. The hash recovered
    /// from a successful `send_transaction` is authoritative.
    ///
    /// Failure semantics (matching `executor::classify`):
    /// - **Terminal RPC error** (revert / nonce / insufficient funds /
    ///   gas validation / signature) → bubble as `AllEndpointsFailed`
    ///   with the node's original message so `classify` routes
    ///   correctly. We do NOT continue to sub endpoints because every
    ///   node would return the same state-driven reject.
    /// - **Transient transport error / timeout** → fall through to the
    ///   next endpoint.
    /// - **AlreadyKnown** before we ever got a hash → returns
    ///   `AllEndpointsFailed` with reason="tx_in_mempool_no_hash" so
    ///   the executor can mark the row `pending` (with new nonce on
    ///   next cycle). Boot reconciliation handles the in-mempool tx
    ///   when its receipt eventually lands. Edge case — only happens if
    ///   a prior endpoint accepted the wire bytes but never returned a
    ///   response.
    pub async fn send_setreceiver(
        &self,
        gift_vault: Address,
        token: Address,
        receiver: Address,
        nonce: u64,
        fees: Fees,
        bot: Address,
    ) -> Result<TxHash, RpcChainError> {
        let calldata = GiftVault::setReceiverCall { token, receiver }.abi_encode();

        let request = TransactionRequest::default()
            .with_from(bot)
            .with_to(gift_vault)
            .with_input(Bytes::from(calldata))
            .with_nonce(nonce)
            .with_max_fee_per_gas(fees.max_fee_per_gas)
            .with_max_priority_fee_per_gas(fees.max_priority_fee_per_gas);

        let mut last_transient = String::from("no endpoints tried");

        for ep in &self.endpoints {
            // Each endpoint's wallet filler signs identically (same
            // signer cloned at boot). `send_transaction` runs gas +
            // chain_id fillers + signing + raw submit in one pass.
            let op = async {
                ep.provider
                    .send_transaction(request.clone())
                    .await
                    .map(|pending| *pending.tx_hash())
                    .map_err(|e| e.to_string())
            };
            match timeout(self.send_timeout, op).await {
                Ok(Ok(hash)) => {
                    info!(url = %ep.url, %hash, "send_transaction accepted");
                    return Ok(hash);
                }
                Ok(Err(msg)) => {
                    let lower = msg.to_ascii_lowercase();
                    if lower.contains("already known") || lower.contains("already in mempool") {
                        // A prior endpoint successfully accepted the tx
                        // (mempool is shared) but the response never
                        // reached us. We don't have the hash on this
                        // path — bubble so the executor can re-mark
                        // pending; boot reconciliation will sweep the
                        // in-mempool tx once it lands.
                        warn!(
                            url = %ep.url,
                            error = %msg,
                            "tx already in mempool but hash unrecovered — letting reconcile handle"
                        );
                        return Err(RpcChainError::AllEndpointsFailed {
                            op: "send_transaction",
                            reason: format!("tx_in_mempool_no_hash: {msg}"),
                        });
                    }
                    if is_terminal_rpc_message(&lower) {
                        // Node reports a state-driven reject. Every
                        // node would say the same — bubble so the
                        // executor's `classify` takes over.
                        warn!(url = %ep.url, error = %msg, "terminal RPC error on send");
                        return Err(RpcChainError::AllEndpointsFailed {
                            op: "send_transaction",
                            reason: msg,
                        });
                    }
                    warn!(
                        url = %ep.url,
                        error = %msg,
                        "send_transaction transient error; trying next endpoint"
                    );
                    last_transient = msg;
                }
                Err(_) => {
                    warn!(
                        url = %ep.url,
                        timeout_ms = self.send_timeout.as_millis() as u64,
                        "send_transaction timed out on endpoint"
                    );
                    last_transient = format!("timeout after {}ms", self.send_timeout.as_millis());
                }
            }
        }
        Err(RpcChainError::AllEndpointsFailed {
            op: "send_transaction",
            reason: last_transient,
        })
    }

    /// Internal: try each endpoint in order with a per-attempt timeout.
    /// Any single failure or timeout logs a warn and advances. All
    /// endpoints failing yields `AllEndpointsFailed`.
    async fn call_read<F, Fut, T>(&self, op: &'static str, f: F) -> Result<T, RpcChainError>
    where
        F: Fn(&Endpoint) -> Fut,
        Fut: std::future::Future<Output = Result<T, String>>,
    {
        let mut last_err = String::from("no endpoints tried");
        for ep in &self.endpoints {
            match timeout(self.read_timeout, f(ep)).await {
                Ok(Ok(v)) => return Ok(v),
                Ok(Err(msg)) => {
                    warn!(url = %ep.url, op, error = %msg, "RPC read failed on endpoint; falling through");
                    last_err = msg;
                }
                Err(_) => {
                    warn!(
                        url = %ep.url,
                        op,
                        timeout_ms = self.read_timeout.as_millis() as u64,
                        "RPC read timed out on endpoint; falling through"
                    );
                    last_err = format!("timeout after {}ms", self.read_timeout.as_millis());
                }
            }
        }
        Err(RpcChainError::AllEndpointsFailed {
            op,
            reason: last_err,
        })
    }
}

/// EIP-1559 fee pair. Small projection of alloy's fee estimate so
/// executor code doesn't need to depend on the alloy fee type.
#[derive(Debug, Clone, Copy)]
pub struct Fees {
    pub max_fee_per_gas: u128,
    pub max_priority_fee_per_gas: u128,
}

/// Heuristic: is this RPC error a state-driven reject that every Monad
/// node would return? If yes, bubble it without trying alternates so
/// the executor's `classify` can take over.
///
/// Conservative — treats anything we don't recognize as transient
/// (continue to next endpoint). Recognized terminals: revert markers,
/// nonce conflicts, insufficient funds, gas validation, signature
/// validation. Operator hardware faults / network blips fall through to
/// the next endpoint as transient.
fn is_terminal_rpc_message(lower: &str) -> bool {
    lower.contains("execution reverted")
        || lower.contains("vm exception")
        || lower.contains("0x08c379a0")
        || lower.contains("0x4e487b71")
        || lower.contains("nonce too low")
        || lower.contains("nonce is too low")
        || lower.contains("invalid nonce")
        || lower.contains("nonce has already been used")
        || lower.contains("insufficient funds")
        || lower.contains("insufficient balance")
        || lower.contains("intrinsic gas too low")
        || lower.contains("gas limit reached")
        || lower.contains("invalid signature")
}

/// Outcome of `probe_transaction`. Drives §6.1 reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TxProbe {
    /// Tx was mined. Caller should look at the receipt and/or on-chain
    /// state to decide `completed` vs `reverted`.
    Mined { block_number: u64 },
    /// Tx is still pending — hold the row as `submitted`.
    PendingInMempool,
    /// No node has the hash anywhere — tx was dropped. Resubmit with a
    /// fresh nonce.
    Unknown,
}

/// Dummy trait import so rustc knows the `U256` / `B256` / `TxHash` aliases
/// resolve on the public signatures above; silences unused-imports when
/// the file is first scaffolded.
#[allow(dead_code)]
fn _type_witnesses(_: U256, _: B256, _: TxHash) {}

#[cfg(test)]
mod tests {
    use super::*;

    // --- is_terminal_rpc_message ---
    //
    // Routing decision in send-failover hinges on this. A miscalibration
    // either (a) tries every endpoint for a hopeless reject (slow + log
    // noise) or (b) bubbles a transient hiccup as terminal (false
    // rejected row). Pin the obvious cases.

    #[test]
    fn terminal_rpc_recognizes_revert() {
        assert!(is_terminal_rpc_message("execution reverted: NotConfigured"));
        assert!(is_terminal_rpc_message("vm exception while processing"));
        assert!(is_terminal_rpc_message("returned data: 0x08c379a0..."));
        assert!(is_terminal_rpc_message("0x4e487b71 panic"));
    }

    #[test]
    fn terminal_rpc_recognizes_nonce_conflict() {
        assert!(is_terminal_rpc_message("nonce too low"));
        assert!(is_terminal_rpc_message("nonce is too low: have 5 want 4"));
        assert!(is_terminal_rpc_message("invalid nonce; expected 7, got 4"));
        assert!(is_terminal_rpc_message(
            "nonce has already been used by tx 0x..."
        ));
    }

    #[test]
    fn terminal_rpc_recognizes_funds_and_gas() {
        assert!(is_terminal_rpc_message(
            "insufficient funds for gas * price"
        ));
        assert!(is_terminal_rpc_message("insufficient balance"));
        assert!(is_terminal_rpc_message("intrinsic gas too low"));
        assert!(is_terminal_rpc_message("gas limit reached"));
        assert!(is_terminal_rpc_message("invalid signature for tx"));
    }

    #[test]
    fn terminal_rpc_falls_through_on_transient() {
        // Transport-shape failures must NOT trigger terminal — these
        // are the cases failover is supposed to recover from.
        assert!(!is_terminal_rpc_message("connection refused"));
        assert!(!is_terminal_rpc_message("connection reset by peer"));
        assert!(!is_terminal_rpc_message("timeout after 10s"));
        assert!(!is_terminal_rpc_message("502 bad gateway"));
        assert!(!is_terminal_rpc_message("eof while parsing response"));
        assert!(!is_terminal_rpc_message(""));
    }

    #[test]
    fn terminal_rpc_does_not_swallow_unrelated_words() {
        // "reverted to fallback" is a transport-level event ("network
        // reverted to fallback codec"), NOT a contract revert. Our
        // marker is "execution reverted" — the more specific phrase.
        assert!(!is_terminal_rpc_message("network reverted to fallback"));
    }

    // Anvil-dependent integration tests. These require the alloy
    // "node-bindings" feature which cannot be added as a dev-dependency
    // yet (tempfile version conflict with redis). Re-enabled in Phase 3
    // (alloy node-bindings dev-dep) once the conflict is resolved.
    #[cfg(feature = "node-bindings")]
    mod anvil_tests {
        use super::*;
        use alloy::node_bindings::Anvil;
        use alloy::signers::local::PrivateKeySigner;

        fn anvil_signer(anvil: &alloy::node_bindings::AnvilInstance) -> PrivateKeySigner {
            anvil.keys()[0].clone().into()
        }

        #[tokio::test]
        async fn boot_rejects_chain_id_mismatch_on_main() {
            let anvil = Anvil::new().try_spawn().expect("anvil required");
            let signer = anvil_signer(&anvil);
            let shutdown = CancellationToken::new();
            let wrong_chain_id = anvil.chain_id() + 9999;
            let result = RpcChain::connect(
                &[anvil.endpoint()],
                wrong_chain_id,
                signer,
                Duration::from_secs(2),
                Duration::from_secs(2),
                &shutdown,
            )
            .await;
            // RpcChain doesn't impl Debug (it holds signer material) so
            // unwrap_err is not available — match explicitly.
            match result {
                Err(RpcChainError::ChainIdMismatch {
                    expected, actual, ..
                }) => {
                    assert_eq!(expected, wrong_chain_id);
                    assert_eq!(actual, anvil.chain_id());
                }
                Err(other) => panic!("expected ChainIdMismatch, got {other:?}"),
                Ok(_) => panic!("boot must fail when chain_id does not match"),
            }
        }

        #[tokio::test]
        async fn boot_rejects_mismatch_on_sub_endpoint() {
            // This is the critical invariant from §7.2: a sub endpoint on the
            // wrong chain must be terminal, not a warning. Use two anvils
            // with different chain_ids.
            let main = Anvil::new()
                .chain_id(1337)
                .try_spawn()
                .expect("anvil required");
            let sub = Anvil::new()
                .chain_id(9999)
                .try_spawn()
                .expect("anvil required");

            let signer = anvil_signer(&main);
            let shutdown = CancellationToken::new();
            let result = RpcChain::connect(
                &[main.endpoint(), sub.endpoint()],
                1337,
                signer,
                Duration::from_secs(2),
                Duration::from_secs(2),
                &shutdown,
            )
            .await;
            match result {
                Err(RpcChainError::ChainIdMismatch { url, actual, .. }) => {
                    assert!(
                        url.contains(&sub.port().to_string()),
                        "mismatch must name the sub URL"
                    );
                    assert_eq!(actual, 9999);
                }
                Err(other) => panic!("expected ChainIdMismatch on sub endpoint, got {other:?}"),
                Ok(_) => panic!("boot must fail when sub endpoint is on a different chain"),
            }
        }

        #[tokio::test]
        async fn connect_succeeds_on_matching_chain_ids() {
            let anvil = Anvil::new().try_spawn().expect("anvil required");
            let signer = anvil_signer(&anvil);
            let shutdown = CancellationToken::new();
            let chain = RpcChain::connect(
                &[anvil.endpoint()],
                anvil.chain_id(),
                signer,
                Duration::from_secs(2),
                Duration::from_secs(2),
                &shutdown,
            )
            .await
            .expect("boot should succeed on matching chain id");
            assert_eq!(chain.endpoint_urls().len(), 1);
        }

        #[tokio::test]
        async fn call_read_falls_through_on_first_endpoint_timeout() {
            // Simulate first-endpoint failure by using a bogus URL that won't
            // connect. Since `build_provider` fails at boot, we construct an
            // RpcChain directly via `for_testing` with a stale provider that
            // errors out on every RPC call, followed by a real working anvil.
            let good = Anvil::new().try_spawn().expect("anvil required");
            let signer = anvil_signer(&good);

            // Build a broken endpoint: HTTP provider pointed at a port nobody
            // is listening on. Unreachable-refused fails fast; the second
            // endpoint (real anvil) answers. We prove fallback by calling
            // `get_nonce_pending` which is just a getTransactionCount.
            let bad_url = "http://127.0.0.1:1".to_string();
            let bad_provider = ProviderBuilder::new()
                .wallet(EthereumWallet::from(signer.clone()))
                .connect(&bad_url)
                .await
                .expect("http provider construction is lazy");
            let bad_ep = Endpoint {
                url: bad_url,
                provider: DynProvider::new(bad_provider),
            };

            let good_provider = ProviderBuilder::new()
                .wallet(EthereumWallet::from(signer.clone()))
                .connect(&good.endpoint())
                .await
                .unwrap();
            let good_ep = Endpoint {
                url: good.endpoint(),
                provider: DynProvider::new(good_provider),
            };

            let chain = RpcChain::for_testing(
                vec![bad_ep, good_ep],
                signer.address(),
                Duration::from_millis(500),
                Duration::from_secs(2),
            );
            let nonce = chain
                .get_nonce_pending()
                .await
                .expect("fallback endpoint should answer");
            // Nothing has been sent yet; a fresh anvil account has nonce 0.
            assert_eq!(nonce, 0);
        }
    }
}
