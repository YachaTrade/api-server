//! Reply-on-success worker — polls `gift_tweet WHERE reply_status='pending'`
//! and posts a "🎁 Gift activated! https://nad.fun/profile/0x…?tab=gift" reply via
//! OAuth 1.0a to X.
//!
//! Decoupled from the on-chain pipeline on purpose: if X is down, gifts
//! still get processed. Worker survives all X-side failures (mark the row
//! `failed` after `max_attempts`, never bubble to the supervisor).
//!
//! ## Loop body
//!
//! 1. Sleep `poll_interval_ms` or wake on shutdown.
//! 2. `claim_one_reply_pending` — SELECT … FOR UPDATE SKIP LOCKED.
//! 3. Substitute `{receiver}` in the template, POST to `/2/tweets`.
//! 4. Outcome:
//!    - Success → `mark_reply_sent(tweet_id, reply_tweet_id)`.
//!    - Retryable error + attempts < max → `bump_reply_attempt`; row
//!      stays pending; backoff before the next poll picks it up.
//!    - Non-retryable error or attempts >= max → `mark_reply_failed`.
//!
//! `attempts` is read **after** the increment for cap checks, matching
//! the user-facing definition "after N tries, give up."

use std::time::Duration;

use reqwest::Client;
use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use crate::services::gift::config::GiftConfig;
use crate::services::gift::db;
use crate::services::gift::db_retry::with_retry;
use crate::services::gift::x_reply::{OAuth1Credentials, ReplyError, post_reply};

/// Default body template. `{receiver}` is substituted with the 0x… address
/// of the gift's receiver. Matches gift-bot's `DEFAULT_REPLY_TEMPLATE`.
const DEFAULT_REPLY_TEMPLATE: &str =
    "🎁 Gift activated! https://nad.fun/profile/{receiver}?tab=gift";
/// Max times the worker will re-attempt a single row before marking
/// `reply_status='failed'`. Matches gift-bot's `DEFAULT_REPLY_MAX_ATTEMPTS`.
const DEFAULT_REPLY_MAX_ATTEMPTS: u32 = 5;
/// How often the reply worker polls for `reply_status='pending'` rows.
/// gift-bot's default was 10_000; api-server's leader poll cadence is
/// faster, so we use 5_000 here per the Phase 4 spec.
const DEFAULT_REPLY_POLL_INTERVAL_MS: u64 = 5_000;
/// gift-bot's `load_reply_config` default for `X_API_BASE`. The trailing
/// slash is omitted — `post_reply` appends `/2/tweets`.
const DEFAULT_API_BASE: &str = "https://api.x.com";

/// Materialised reply-worker config. Built from `GiftConfig` via
/// [`ReplyConfig::from_gift`].
#[derive(Clone)]
pub struct ReplyConfig {
    pub poll_interval_ms: u64,
    pub max_attempts: u32,
    /// Body template with `{receiver}` placeholder.
    pub template: String,
    pub creds: OAuth1Credentials,
    pub api_base: String,
}

impl std::fmt::Debug for ReplyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplyConfig")
            .field("poll_interval_ms", &self.poll_interval_ms)
            .field("max_attempts", &self.max_attempts)
            .field("template", &self.template)
            .field("creds", &self.creds)
            .field("api_base", &self.api_base)
            .finish()
    }
}

impl ReplyConfig {
    /// Build a reply config from the shared `GiftConfig`. The four OAuth
    /// secrets (also used for CRC + webhook signature) are reused for
    /// reply signing; the rest are Phase-4 defaults matching gift-bot's
    /// `load_reply_config`.
    pub fn from_gift(config: &GiftConfig) -> Self {
        Self {
            poll_interval_ms: DEFAULT_REPLY_POLL_INTERVAL_MS,
            max_attempts: DEFAULT_REPLY_MAX_ATTEMPTS,
            template: config
                .reply_template
                .clone()
                .unwrap_or_else(|| DEFAULT_REPLY_TEMPLATE.to_string()),
            creds: OAuth1Credentials {
                consumer_key: config.oauth_consumer_key.clone(),
                consumer_secret: config.oauth_consumer_secret.clone(),
                access_token: config.oauth_access_token.clone(),
                access_token_secret: config.oauth_access_token_secret.clone(),
            },
            api_base: DEFAULT_API_BASE.to_string(),
        }
    }
}

/// Long-lived reply worker. Spawned once from the leader's `run_leader`
/// when `GIFT_X_REPLY_ENABLED` is set.
pub struct ReplyWorker {
    pub pool: PgPool,
    pub config: ReplyConfig,
    pub http: Client,
}

impl ReplyWorker {
    pub async fn run(self, shutdown: CancellationToken) {
        info!(
            poll_interval_ms = self.config.poll_interval_ms,
            max_attempts = self.config.max_attempts,
            "reply worker starting"
        );

        let poll_interval = Duration::from_millis(self.config.poll_interval_ms);

        loop {
            if shutdown.is_cancelled() {
                info!("shutdown observed, exiting reply worker");
                return;
            }

            // Sleep first so we don't hammer the DB if pool is hot.
            // Shutdown-aware: wake immediately on cancel.
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => {
                    info!("shutdown during sleep, exiting reply worker");
                    return;
                }
                _ = tokio::time::sleep(poll_interval) => {}
            }

            // Claim one row.
            let row = match with_retry("claim_one_reply_pending", || async {
                db::claim_one_reply_pending(&self.pool)
                    .await
                    .map_err(Into::into)
            })
            .await
            {
                Ok(Some(r)) => r,
                Ok(None) => continue,
                Err(e) => {
                    warn!(error = %e, "reply worker: DB claim failed; will retry next tick");
                    continue;
                }
            };

            // Build reply text via template.
            let receiver_str = format!("{:#x}", row.receiver_id);
            let text = self.config.template.replace("{receiver}", &receiver_str);

            // Post to X.
            match post_reply(
                &self.http,
                &self.config.creds,
                &self.config.api_base,
                &text,
                &row.tweet_id,
            )
            .await
            {
                Ok(posted) => {
                    info!(
                        tweet_id = %row.tweet_id,
                        reply_tweet_id = %posted.id,
                        "reply sent"
                    );
                    crate::metrics::METRICS.gift.record_reply_success();
                    let _ = with_retry("mark_reply_sent", || async {
                        db::mark_reply_sent(&self.pool, &row.tweet_id, &posted.id)
                            .await
                            .map_err(Into::into)
                    })
                    .await;
                }
                Err(e) => {
                    let next_attempts = (row.attempts as u32).saturating_add(1);
                    let retryable = e.is_retryable();
                    let give_up = !retryable || next_attempts >= self.config.max_attempts;

                    if give_up {
                        warn!(
                            tweet_id = %row.tweet_id,
                            attempts = next_attempts,
                            retryable,
                            error = %e,
                            "reply terminally failed; marking row reply_status='failed'"
                        );
                        // Terminal failure — bump the streak so a sustained
                        // OAuth-permissions outage (Forbidden / Unauthorized)
                        // shows up as `gift_bot_reply_consecutive_failures`
                        // climbing on every claimed row.
                        crate::metrics::METRICS.gift.record_reply_failure();
                        let err_str = e.to_string();
                        let _ = with_retry("mark_reply_failed", || async {
                            db::mark_reply_failed(&self.pool, &row.tweet_id, &err_str)
                                .await
                                .map_err(Into::into)
                        })
                        .await;
                    } else {
                        warn!(
                            tweet_id = %row.tweet_id,
                            attempts = next_attempts,
                            error = %e,
                            "reply attempt failed; will retry next tick"
                        );
                        let err_str = e.to_string();
                        let _ = with_retry("bump_reply_attempt", || async {
                            db::bump_reply_attempt(&self.pool, &row.tweet_id, &err_str)
                                .await
                                .map_err(Into::into)
                        })
                        .await;

                        // On rate-limit specifically, sleep the X-told
                        // window before the next poll so we don't burn
                        // attempts against a known-locked endpoint.
                        if let ReplyError::RateLimited { retry_after_s, .. } = &e {
                            let backoff = Duration::from_secs(*retry_after_s);
                            tokio::select! {
                                biased;
                                _ = shutdown.cancelled() => {
                                    info!("shutdown during rate-limit sleep, exiting reply worker");
                                    return;
                                }
                                _ = tokio::time::sleep(backoff) => {}
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloy::primitives::address;

    #[test]
    fn template_substitutes_lowercase_hex_receiver() {
        // The substituted address must be lowercase `0x` + 40 hex — that's
        // what the nad.fun profile URL expects, and EIP-55 checksum
        // capitalization would surprise users following the link.
        let receiver = address!("AAAAaaaaAAAAaaaaAAAAaaaaAAAAaaaaAAAAaaaa");
        let templated = "🎁 https://nad.fun/profile/{receiver}"
            .replace("{receiver}", &format!("{:#x}", receiver));
        assert_eq!(
            templated,
            "🎁 https://nad.fun/profile/0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    #[test]
    fn template_handles_zero_address_explicitly() {
        // Zero receivers should never reach the reply worker (preflight
        // rejects them earlier), but the substitution itself shouldn't
        // panic if it happens to.
        let receiver = alloy::primitives::Address::ZERO;
        let templated = "{receiver}".replace("{receiver}", &format!("{:#x}", receiver));
        assert_eq!(templated, "0x0000000000000000000000000000000000000000");
    }
}
