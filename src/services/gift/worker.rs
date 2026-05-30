//! Single-leader gift consumer. Every api-server instance calls spawn(); only
//! the pg_advisory_lock winner runs the on-chain consumer loop. Webhook ingest
//! runs on all instances regardless. Boot sequence mirrors gift-bot
//! consumer/main.rs (telemetry/health/metrics/reply deferred; reply in Phase 4).
use std::time::Duration;

use sqlx::PgPool;
use tokio::time::interval;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use crate::chain::rpc_chain::{RpcChain, RpcChainError};
use crate::config::GIFT_CONSUMER_LOCK_KEY;
use crate::services::gift::config::GiftConfig;
use crate::services::gift::db;
use crate::services::gift::executor::{Executor, ExecutorConfig};
use crate::services::gift::poller::Poller;
use crate::state::AppState;

const RECONTEND_INTERVAL: Duration = Duration::from_secs(15);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);

/// Spawn the leader-election + consumer task. Non-blocking. No-op (with a log)
/// if gift runtime is absent (GIFT_* unconfigured) — webhook stays degraded.
pub fn spawn(state: AppState) {
    let Some(gift) = state.gift.clone() else {
        info!("gift consumer worker not started: gift runtime absent (GIFT_* unconfigured)");
        return;
    };
    if gift.config.paused {
        info!("gift consumer worker not started: GIFT_PAUSED=true");
        return;
    }
    let pool = state.postgres.get_write_pool().clone();
    tokio::spawn(async move {
        let mut tick = interval(RECONTEND_INTERVAL);
        loop {
            tick.tick().await;
            match acquire_leader_conn(&pool).await {
                Ok(Some(lock_conn)) => {
                    info!("gift consumer: acquired advisory lock — running as leader");
                    let session = CancellationToken::new();
                    // heartbeat: if the lock connection dies, cancel session so
                    // the poller stops and we re-contend (bounds double-leader).
                    let hb = spawn_heartbeat(lock_conn, session.clone());
                    if let Err(e) = run_leader(&gift.config, pool.clone(), session.clone()).await {
                        error!(error = %e, "gift consumer leader loop errored; re-contending");
                    }
                    session.cancel();
                    let _ = hb.await; // drops the lock connection → releases advisory lock
                    info!("gift consumer: released leadership; will re-contend");
                }
                Ok(None) => { /* another instance leads; retry next tick */ }
                Err(e) => warn!(error = %e, "gift consumer: advisory lock attempt failed"),
            }
        }
    });
}

/// Acquire a dedicated connection and try the advisory lock on it. Returns the
/// held connection if we won; None if another instance holds it.
async fn acquire_leader_conn(
    pool: &PgPool,
) -> Result<Option<sqlx::pool::PoolConnection<sqlx::Postgres>>, sqlx::Error> {
    let mut conn = pool.acquire().await?;
    let got: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(GIFT_CONSUMER_LOCK_KEY)
        .fetch_one(&mut *conn)
        .await?;
    Ok(if got { Some(conn) } else { None })
}

/// Heartbeat task owning the lock connection. Pings every HEARTBEAT_INTERVAL;
/// on failure (connection dropped) cancels `session` so the leader stops, then
/// returns (dropping the connection releases the advisory lock).
fn spawn_heartbeat(
    mut lock_conn: sqlx::pool::PoolConnection<sqlx::Postgres>,
    session: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut beat = interval(HEARTBEAT_INTERVAL);
        loop {
            tokio::select! {
                _ = session.cancelled() => break,
                _ = beat.tick() => {
                    if let Err(e) = sqlx::query("SELECT 1").execute(&mut *lock_conn).await {
                        warn!(error = %e, "gift consumer: lock connection lost; abdicating leadership");
                        session.cancel();
                        break;
                    }
                }
            }
        }
        // lock_conn dropped here → pg advisory lock released.
    })
}

/// One leadership term: replicate consumer/main.rs boot sequence, then poll
/// until `session` is cancelled. Returns Err on boot failure (we re-contend).
async fn run_leader(
    config: &GiftConfig,
    pool: PgPool,
    session: CancellationToken,
) -> anyhow::Result<()> {
    use anyhow::{Context, anyhow};

    let reply_enabled = config.reply_enabled;

    let signer: alloy::signers::local::PrivateKeySigner = config
        .bot_private_key
        .parse()
        .context("GIFT_BOT_PRIVATE_KEY parse failed (config already validated shape)")?;
    info!(bot = %signer.address(), "gift consumer: bot signer loaded");

    let chain = match RpcChain::connect(
        &config.rpc_endpoints(),
        config.monad_chain_id,
        signer,
        Duration::from_millis(config.rpc_read_timeout_ms),
        Duration::from_millis(config.rpc_send_timeout_ms),
        &session,
    )
    .await
    {
        Ok(c) => c,
        Err(RpcChainError::Cancelled) => {
            info!("gift consumer: shutdown during RPC boot");
            return Ok(());
        }
        Err(e) => return Err(anyhow!(e)).context("RpcChain connect failed"),
    };
    info!(bot = %chain.signer_address(), "gift consumer: RpcChain ready (chain_id verified)");

    // Ensure the Account Activity (AAA) subscription exists so X delivers events
    // to our webhook. gift-bot's producer reconciled stream rules at boot; this
    // is the webhook equivalent — idempotent, runs only when GIFT_X_WEBHOOK_ID is
    // set. The logged status doubles as a diagnostic (204=ok, 403=no AAA access,
    // 401=token not the subscribed user).
    if let Some(webhook_id) = config.webhook_id.as_deref() {
        let creds = crate::services::gift::x_reply::OAuth1Credentials {
            consumer_key: config.oauth_consumer_key.clone(),
            consumer_secret: config.oauth_consumer_secret.clone(),
            access_token: config.oauth_access_token.clone(),
            access_token_secret: config.oauth_access_token_secret.clone(),
        };
        match reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
        {
            Ok(http) => {
                match crate::services::gift::x_reply::ensure_subscription(
                    &http,
                    &creds,
                    "https://api.x.com",
                    webhook_id,
                )
                .await
                {
                    Ok((status, _body)) if (200..300).contains(&status) => {
                        info!(status, webhook_id, "gift consumer: AAA subscription ensured");
                    }
                    Ok((status, body)) => {
                        warn!(status, webhook_id, body = %body,
                            "gift consumer: AAA subscription returned non-2xx (403=no AAA access, 401=wrong token)");
                    }
                    Err(e) => warn!(error = %e, "gift consumer: AAA subscription call failed"),
                }
            }
            Err(e) => warn!(error = %e, "gift consumer: failed to build http client for subscription"),
        }
    } else {
        info!("gift consumer: GIFT_X_WEBHOOK_ID unset — skipping AAA auto-subscribe (manual)");
    }

    let exec_cfg = ExecutorConfig {
        dry_run: config.dry_run,
        tx_confirmations: config.tx_confirmations,
        tx_graceful_drain_ms: config.tx_graceful_drain_ms,
        gift_vault: config.gift_vault_address,
        reply_enabled,
    };
    let executor = Executor::new(chain.clone(), pool.clone(), exec_cfg);

    // boot reconciliation sweep (§8.2)
    match db::list_submitted(&pool).await {
        Ok(rows) => {
            info!(count = rows.len(), "gift consumer: boot reconciliation sweep starting");
            for row in rows {
                if session.is_cancelled() {
                    break;
                }
                match executor.reconcile_submitted_row(&row).await {
                    Ok(outcome) => {
                        info!(tweet_id = %row.tweet_id, ?outcome, "boot reconcile complete")
                    }
                    Err(e) => {
                        error!(tweet_id = %row.tweet_id, error = %e, "boot reconcile failed; will retry")
                    }
                }
            }
        }
        Err(e) => warn!(error = %e, "gift consumer: boot sweep list_submitted failed; continuing"),
    }

    let listen_url = std::env::var("PRIMARY_DATABASE_URL")
        .context("PRIMARY_DATABASE_URL must be set for gift consumer LISTEN")?;
    let poller = Poller {
        pool: pool.clone(),
        chain,
        executor,
        gift_vault: config.gift_vault_address,
        poll_interval: Duration::from_millis(config.poll_interval_ms),
        database_url: listen_url,
        reply_enabled,
        consumer_wait_time: Duration::from_millis(config.consumer_wait_time_ms),
    };

    // Reply-on-success worker — spawned only when GIFT_X_REPLY_ENABLED=true.
    // Reuses the four GIFT_X_OAUTH_* secrets for OAuth 1.0a signing. Runs
    // alongside the poll loop for the duration of this leadership term and
    // is cancelled via the shared session token.
    let reply_handle = if reply_enabled {
        let worker = crate::services::gift::reply::ReplyWorker {
            pool: pool.clone(),
            config: crate::services::gift::reply::ReplyConfig::from_gift(config),
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .context("reqwest client build failed for reply worker")?,
        };
        let s = session.clone();
        info!("gift consumer: reply worker enabled");
        Some(tokio::spawn(async move { worker.run(s).await }))
    } else {
        info!("gift consumer: reply worker disabled (GIFT_X_REPLY_ENABLED not set)");
        None
    };

    info!("gift consumer: entering poll loop");
    poller.run(session).await;
    info!("gift consumer: poll loop exited");

    if let Some(h) = reply_handle {
        let _ = h.await;
    }
    Ok(())
}
