//! GiftConfig — all gift-bot config consolidated under the GIFT_ env prefix.
//! Field semantics match gift-bot's ProducerConfig + ConsumerConfig; only the
//! env var names gain a `GIFT_` prefix to avoid api-server collisions.
use std::str::FromStr;

use alloy::primitives::Address;

#[derive(Debug, Clone)]
pub struct GiftConfig {
    // X webhook + OAuth 1.0a (CRC/signature/reply 공용)
    pub oauth_consumer_key: String,
    pub oauth_consumer_secret: String,
    pub oauth_access_token: String,
    pub oauth_access_token_secret: String,
    pub webhook_id: String,
    // parser slots
    pub mention_account: String,
    pub activation_prefix: String,
    pub recipient_prefix: String,
    pub required_hashtag: String,
    // chain (consumer)
    pub main_rpc_url: String,
    pub sub_rpc_url_1: Option<String>,
    pub sub_rpc_url_2: Option<String>,
    pub monad_chain_id: u64,
    pub gift_vault_address: Address,
    pub bot_private_key: String,
    pub poll_interval_ms: u64,
    pub tx_graceful_drain_ms: u64,
    pub rpc_read_timeout_ms: u64,
    pub rpc_send_timeout_ms: u64,
    pub tx_confirmations: u64,
    pub consumer_wait_time_ms: u64,
    pub dry_run: bool,
    pub paused: bool,
    /// Reply-on-success worker toggle (`GIFT_X_REPLY_ENABLED`, default false).
    /// When true the leader spawns the OAuth1 reply worker, which reuses the
    /// four `GIFT_X_OAUTH_*` secrets for signing.
    pub reply_enabled: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum GiftConfigError {
    #[error("missing required env var: {0}")]
    Missing(&'static str),
    #[error("invalid value for {0}: {1}")]
    Invalid(&'static str, String),
}

impl GiftConfig {
    /// Load from process environment.
    pub fn load() -> Result<Self, GiftConfigError> {
        Self::from_getter(|k| std::env::var(k).ok())
    }

    /// Pure loader: `get` returns the value for an env key, or None. Used by
    /// tests to avoid mutating global process env.
    pub fn from_getter(get: impl Fn(&str) -> Option<String>) -> Result<Self, GiftConfigError> {
        let req = |k: &'static str| {
            get(k)
                .filter(|s| !s.is_empty())
                .ok_or(GiftConfigError::Missing(k))
        };
        let gift_vault_address = Address::from_str(&req("GIFT_VAULT_ADDRESS")?)
            .map_err(|e| GiftConfigError::Invalid("GIFT_VAULT_ADDRESS", e.to_string()))?;
        Ok(Self {
            oauth_consumer_key: req("GIFT_X_OAUTH_CONSUMER_KEY")?,
            oauth_consumer_secret: req("GIFT_X_OAUTH_CONSUMER_SECRET")?,
            oauth_access_token: req("GIFT_X_OAUTH_ACCESS_TOKEN")?,
            oauth_access_token_secret: req("GIFT_X_OAUTH_ACCESS_TOKEN_SECRET")?,
            webhook_id: req("GIFT_X_WEBHOOK_ID")?,
            mention_account: req("GIFT_X_MENTION_ACCOUNT")?,
            activation_prefix: req("GIFT_X_ACTIVATION_PREFIX")?,
            recipient_prefix: req("GIFT_X_RECIPIENT_PREFIX")?,
            required_hashtag: req("GIFT_X_REQUIRED_HASHTAG")?,
            main_rpc_url: req("GIFT_MAIN_RPC_URL")?,
            sub_rpc_url_1: get("GIFT_SUB_RPC_URL_1").filter(|s| !s.is_empty()),
            sub_rpc_url_2: get("GIFT_SUB_RPC_URL_2").filter(|s| !s.is_empty()),
            monad_chain_id: req("GIFT_MONAD_CHAIN_ID")?
                .parse()
                .map_err(|e: std::num::ParseIntError| {
                    GiftConfigError::Invalid("GIFT_MONAD_CHAIN_ID", e.to_string())
                })?,
            gift_vault_address,
            bot_private_key: req("GIFT_BOT_PRIVATE_KEY")?,
            poll_interval_ms: get("GIFT_CONSUMER_POLL_INTERVAL_MS")
                .and_then(|s| s.parse().ok())
                .unwrap_or(2000),
            tx_graceful_drain_ms: get("GIFT_TX_GRACEFUL_DRAIN_MS")
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000),
            rpc_read_timeout_ms: get("GIFT_RPC_READ_TIMEOUT_MS")
                .and_then(|s| s.parse().ok())
                .unwrap_or(3000),
            rpc_send_timeout_ms: get("GIFT_RPC_SEND_TIMEOUT_MS")
                .and_then(|s| s.parse().ok())
                .unwrap_or(10000),
            tx_confirmations: get("GIFT_TX_CONFIRMATIONS")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1),
            consumer_wait_time_ms: get("GIFT_CONSUMER_WAIT_TIME_MS")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            dry_run: get("GIFT_DRY_RUN").map(|v| v == "true").unwrap_or(false),
            paused: get("GIFT_PAUSED").map(|v| v == "true").unwrap_or(false),
            reply_enabled: get("GIFT_X_REPLY_ENABLED")
                .map(|v| v == "true")
                .unwrap_or(false),
        })
    }

    /// RPC endpoints in failover order: [main, sub1?, sub2?].
    pub fn rpc_endpoints(&self) -> Vec<String> {
        let mut v = vec![self.main_rpc_url.clone()];
        if let Some(u) = &self.sub_rpc_url_1 {
            v.push(u.clone());
        }
        if let Some(u) = &self.sub_rpc_url_2 {
            v.push(u.clone());
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn full_env() -> HashMap<&'static str, &'static str> {
        let mut m = HashMap::new();
        m.insert("GIFT_X_OAUTH_CONSUMER_KEY", "ck_test");
        m.insert("GIFT_X_OAUTH_CONSUMER_SECRET", "cs_test");
        m.insert("GIFT_X_OAUTH_ACCESS_TOKEN", "at_test");
        m.insert("GIFT_X_OAUTH_ACCESS_TOKEN_SECRET", "ats_test");
        m.insert("GIFT_X_WEBHOOK_ID", "wh_test");
        m.insert("GIFT_X_MENTION_ACCOUNT", "nadfunnews");
        m.insert("GIFT_X_ACTIVATION_PREFIX", "Activating Gift for");
        m.insert("GIFT_X_RECIPIENT_PREFIX", "Fees will go to");
        m.insert("GIFT_X_REQUIRED_HASHTAG", "#Nadfun");
        m.insert("GIFT_MAIN_RPC_URL", "https://rpc.example.com");
        m.insert("GIFT_MONAD_CHAIN_ID", "10143");
        m.insert(
            "GIFT_VAULT_ADDRESS",
            "0x0000000000000000000000000000000000000001",
        );
        m.insert("GIFT_BOT_PRIVATE_KEY", "0xdeadbeef");
        m
    }

    #[test]
    fn missing_required_returns_error() {
        let result = GiftConfig::from_getter(|_| None);
        assert!(matches!(result, Err(GiftConfigError::Missing(_))));
    }

    #[test]
    fn loads_full_valid_config() {
        let env = full_env();
        let result = GiftConfig::from_getter(|k| env.get(k).map(|v| v.to_string()));
        let cfg = result.expect("should load valid config");
        assert_eq!(cfg.monad_chain_id, 10143);
        assert!(!cfg.dry_run);
        assert!(cfg.sub_rpc_url_1.is_none());
        assert!(cfg.sub_rpc_url_2.is_none());
        assert_eq!(cfg.poll_interval_ms, 2000);
        assert_eq!(cfg.tx_graceful_drain_ms, 10000);
        assert_eq!(cfg.rpc_read_timeout_ms, 3000);
        assert_eq!(cfg.rpc_send_timeout_ms, 10000);
        assert_eq!(cfg.tx_confirmations, 1);
        assert_eq!(cfg.consumer_wait_time_ms, 0);
        assert!(!cfg.reply_enabled);
        assert_eq!(cfg.mention_account, "nadfunnews");
        assert_eq!(cfg.rpc_endpoints(), vec!["https://rpc.example.com"]);
    }

    #[test]
    fn invalid_chain_id_is_invalid_error() {
        let mut env = full_env();
        env.insert("GIFT_MONAD_CHAIN_ID", "notanum");
        let result = GiftConfig::from_getter(|k| env.get(k).map(|v| v.to_string()));
        match result {
            Err(GiftConfigError::Invalid(key, _)) => assert_eq!(key, "GIFT_MONAD_CHAIN_ID"),
            other => panic!("expected Invalid error, got: {:?}", other),
        }
    }
}
