use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::json;
use tracing::warn;

use crate::services::gift::{crc, ingest, payload::ActivityPayload, signature};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct CrcQuery {
    pub crc_token: String,
}

/// GET /gift/webhook?crc_token=… → { "response_token": "sha256=…" }
pub async fn crc_handler(
    State(state): State<AppState>,
    Query(q): Query<CrcQuery>,
) -> impl IntoResponse {
    match &state.gift {
        Some(g) => {
            crate::metrics::METRICS.gift.inc_crc();
            Json(json!({
                "response_token": crc::response_token(&g.config.oauth_consumer_secret, &q.crc_token)
            }))
            .into_response()
        }
        None => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

/// POST /gift/webhook — verify HMAC signature over raw body, parse, insert.
pub async fn event_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let Some(g) = &state.gift else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let sig = headers
        .get("x-twitter-webhooks-signature")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !signature::verify(&g.config.oauth_consumer_secret, &body, sig) {
        crate::metrics::METRICS.gift.inc_signature_failure();
        warn!("gift webhook signature verification failed");
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let payload: ActivityPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(e) => {
            warn!(error = %e, "gift webhook payload deserialize failed");
            return StatusCode::OK.into_response();
        }
    };
    // Signature-verified + parsed batch.
    crate::metrics::METRICS.gift.inc_webhook_event();
    let pool = state.postgres.get_write_pool();
    let n = ingest::ingest(pool, &g.parser, &payload).await;
    crate::metrics::METRICS.gift.add_ingested(n as u64);
    if n > 0 {
        tracing::info!(inserted = n, "gift webhook batch ingested");
    }
    StatusCode::OK.into_response()
}

/// GET /gift/healthz — liveness for haproxy backend.
pub async fn healthz_handler() -> impl IntoResponse {
    (StatusCode::OK, "ok")
}
