use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapricornPosition {
    pub owner: String,
    pub token0: String,
    pub token1: String,
    /// Raw on-chain amount (wei) for token0, same decimal scale as the `balance` table.
    /// NOTE: this is Capricorn's raw `amount0`, NOT `amount0Human` — `balance` and the
    /// V2 lp formula are raw, so V1 LP must be raw too or `total_balance` units mismatch.
    pub amount0: String,
    /// Raw on-chain amount (wei) for token1 (see `amount0`).
    pub amount1: String,
}

impl CapricornPosition {
    /// Returns the raw on-chain amount of whichever side matches token_id (case-insensitive).
    /// Returns None if neither token side matches.
    pub fn amount_for_token(&self, token_id: &str) -> Option<String> {
        let t = token_id.to_lowercase();
        if self.token0.to_lowercase() == t {
            Some(self.amount0.clone())
        } else if self.token1.to_lowercase() == t {
            Some(self.amount1.clone())
        } else {
            None
        }
    }
}

/// Hard cap on injected LP rows per request. Above this, the caller falls back to
/// enrich-only (no row injection) to keep union sort/pagination bounded (spec §214).
pub const CAPRICORN_UNION_CAP: usize = 2000;

fn parse_amt(s: &str) -> bigdecimal::BigDecimal {
    use std::str::FromStr;
    bigdecimal::BigDecimal::from_str(s).unwrap_or_else(|_| bigdecimal::BigDecimal::from(0))
}

/// For account endpoints: every (nadfun-token-address, summed lp amount) the owner holds LP in.
/// Both pool sides are emitted; addresses that fail EIP-55 parsing (e.g. WMON pseudo-addrs or
/// quote tokens) are dropped — the SQL JOIN to our `token` table filters non-nadfun tokens anyway.
/// Output keys are EIP-55 checksum.
pub fn lp_amounts_by_token(
    positions: &[CapricornPosition],
) -> Vec<(String, bigdecimal::BigDecimal)> {
    use std::collections::HashMap;
    let mut acc: HashMap<String, bigdecimal::BigDecimal> = HashMap::new();
    for p in positions {
        for (addr, amt) in [(&p.token0, &p.amount0), (&p.token1, &p.amount1)] {
            if let Some(checksum) = crate::utils::valid_account_id(addr) {
                *acc.entry(checksum)
                    .or_insert_with(|| bigdecimal::BigDecimal::from(0)) += parse_amt(amt);
            }
        }
    }
    acc.into_iter().collect()
}

/// For the holder endpoint: every (owner, summed lp amount of `token_id`) for one token.
/// Owner addresses are EIP-55 checksummed; invalid owners are dropped.
pub fn lp_amounts_by_owner(
    positions: &[CapricornPosition],
    token_id: &str,
) -> Vec<(String, bigdecimal::BigDecimal)> {
    use std::collections::HashMap;
    let mut acc: HashMap<String, bigdecimal::BigDecimal> = HashMap::new();
    for p in positions {
        let Some(amt) = p.amount_for_token(token_id) else {
            continue;
        };
        let Some(owner_cs) = crate::utils::valid_account_id(&p.owner) else {
            continue;
        };
        *acc.entry(owner_cs)
            .or_insert_with(|| bigdecimal::BigDecimal::from(0)) += parse_amt(&amt);
    }
    acc.into_iter().collect()
}

pub struct CapricornClient {
    http: reqwest::Client,
    url: String,
}

impl CapricornClient {
    pub fn new(url: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .expect("build reqwest client");
        Self { http, url }
    }

    /// Fetches all active positions for the given owner wallet address (paginated).
    /// Pages via `offset` so owners with >500 positions are fully collected (the union
    /// cap is applied by the caller on the complete set, never by silent truncation here).
    /// Returns an empty Vec on any failure (network error, timeout, non-200, GraphQL errors, parse failure).
    pub async fn fetch_by_owner(&self, owner: &str) -> Vec<CapricornPosition> {
        let owner = owner.to_lowercase();
        let mut out = Vec::new();
        let mut offset = 0i64;
        loop {
            let query = format!(
                r#"{{ positions(where: {{ owner: {{ _eq: "{owner}" }} }}, activeOnly: true, limit: 500, offset: {offset}) {{ positions {{ owner token0 {{ tokenAddress }} token1 {{ tokenAddress }} amount0 amount1 }} }} }}"#
            );
            let page = self.post(query).await;
            let n = page.len();
            out.extend(page);
            if n < 500 {
                break;
            }
            offset += 500;
            if offset > 50_000 {
                break; // safety cap
            }
        }
        out
    }

    /// Fetches all active positions for a given token address (paginated).
    /// Only `token0Addresses` is sent: a single address arg behaves OR-like and returns every
    /// pool containing the token regardless of side. Sending both `token0Addresses` AND
    /// `token1Addresses` would AND them (token0==t AND token1==t), which is empty for normal pairs.
    /// Returns an empty Vec on any failure.
    pub async fn fetch_by_token(&self, token_addr: &str) -> Vec<CapricornPosition> {
        let t = token_addr.to_lowercase();
        let mut out = Vec::new();
        let mut offset = 0i64;
        loop {
            let query = format!(
                r#"{{ positions(token0Addresses: ["{t}"], activeOnly: true, limit: 500, offset: {offset}) {{ positions {{ owner token0 {{ tokenAddress }} token1 {{ tokenAddress }} amount0 amount1 }} }} }}"#
            );
            let page = self.post(query).await;
            let n = page.len();
            out.extend(page);
            if n < 500 {
                break;
            }
            offset += 500;
            if offset > 50_000 {
                break; // safety cap
            }
        }
        out
    }

    /// Cached version of `fetch_by_owner`. Uses the global 1-second single-flight cache.
    /// Returns an empty Vec on any failure (same semantics as `fetch_by_owner`).
    pub async fn cached_fetch_by_owner(self: &Arc<Self>, owner: &str) -> Vec<CapricornPosition> {
        use crate::utils::single_flight::{GLOBAL_CACHE, with_cache};
        let key = format!("capricorn:owner:{}", owner.to_lowercase());
        let client = Arc::clone(self);
        let owner_owned = owner.to_string();
        match with_cache(&GLOBAL_CACHE.cache, key, move || {
            let client = Arc::clone(&client);
            async move { Ok(client.fetch_by_owner(&owner_owned).await) }
        })
        .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("capricorn cached_fetch_by_owner failed: {e}");
                Vec::new()
            }
        }
    }

    /// Cached version of `fetch_by_token`. Uses the global 1-second single-flight cache.
    /// Returns an empty Vec on any failure (same semantics as `fetch_by_token`).
    pub async fn cached_fetch_by_token(
        self: &Arc<Self>,
        token_addr: &str,
    ) -> Vec<CapricornPosition> {
        use crate::utils::single_flight::{GLOBAL_CACHE, with_cache};
        let key = format!("capricorn:token:{}", token_addr.to_lowercase());
        let client = Arc::clone(self);
        let token_owned = token_addr.to_string();
        match with_cache(&GLOBAL_CACHE.cache, key, move || {
            let client = Arc::clone(&client);
            async move { Ok(client.fetch_by_token(&token_owned).await) }
        })
        .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("capricorn cached_fetch_by_token failed: {e}");
                Vec::new()
            }
        }
    }

    /// Calls do_post and swallows any error, logging a warning and returning empty Vec.
    async fn post(&self, query: String) -> Vec<CapricornPosition> {
        match self.do_post(query).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("capricorn fetch failed: {e}");
                Vec::new()
            }
        }
    }

    /// Performs the actual HTTP POST to the GraphQL endpoint.
    /// Returns Err on network failure, non-success status, GraphQL errors field, or parse failure.
    async fn do_post(&self, query: String) -> anyhow::Result<Vec<CapricornPosition>> {
        let body = serde_json::json!({ "query": query });
        let resp = self.http.post(&self.url).json(&body).send().await?;
        let v: serde_json::Value = resp.json().await?;
        if v.get("errors").is_some() {
            anyhow::bail!("graphql errors: {}", v["errors"]);
        }
        let arr = v["data"]["positions"]["positions"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(arr
            .into_iter()
            .map(|p| CapricornPosition {
                owner: p["owner"].as_str().unwrap_or_default().to_string(),
                token0: p["token0"]["tokenAddress"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                token1: p["token1"]["tokenAddress"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                amount0: p["amount0"].as_str().unwrap_or("0").to_string(),
                amount1: p["amount1"].as_str().unwrap_or("0").to_string(),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Minimal mock HTTP server (no external crates) ─────────────────────────
    //
    // Spawns a tokio TCP listener on 127.0.0.1:0 (OS-assigned port).
    // Accepts one connection, reads past the request headers, and writes a
    // canned HTTP/1.1 response, then drops.
    //
    // Usage:
    //   let (url, _guard) = mock_server(200, "body").await;
    //   // _guard keeps the server alive via AbortHandle; drops when test ends
    //   let c = CapricornClient::new(url);

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    struct MockServerGuard {
        handle: tokio::task::JoinHandle<()>,
    }
    impl Drop for MockServerGuard {
        fn drop(&mut self) {
            self.handle.abort();
        }
    }

    /// Spawn a mock HTTP/1.1 server that responds to every POST with the given
    /// status code and body.  Returns (url, guard).  The guard keeps the server
    /// alive; drop it to shut down.
    async fn mock_server(status: u16, body: &'static str) -> (String, MockServerGuard) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("http://{}", addr);

        let handle = tokio::spawn(async move {
            // Accept connections in a loop so each call within fetch_by_token
            // (which may make multiple requests for pagination) can be handled.
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                let body = body;
                tokio::spawn(async move {
                    // Drain the incoming HTTP request (read until \r\n\r\n)
                    let mut buf = vec![0u8; 4096];
                    let mut total = 0usize;
                    loop {
                        match stream.read(&mut buf[total..]).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                total += n;
                                if buf[..total].windows(4).any(|w| w == b"\r\n\r\n") {
                                    break;
                                }
                                if total >= buf.len() {
                                    break;
                                }
                            }
                        }
                    }
                    // Write HTTP response
                    let resp = format!(
                        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
                        status = status,
                        reason = if status == 200 {
                            "OK"
                        } else {
                            "Internal Server Error"
                        },
                        len = body.len(),
                    );
                    let _ = stream.write_all(resp.as_bytes()).await;
                });
            }
        });

        (url, MockServerGuard { handle })
    }

    // ── fail-to-empty: HTTP 500 ───────────────────────────────────────────────

    #[tokio::test]
    async fn fetch_by_owner_returns_empty_on_http_500() {
        let (url, _guard) = mock_server(500, "").await;
        let client = CapricornClient::new(url);
        let result = client.fetch_by_owner("0xABCDEF").await;
        assert!(
            result.is_empty(),
            "expected empty vec on HTTP 500, got {} positions",
            result.len()
        );
    }

    #[tokio::test]
    async fn fetch_by_token_returns_empty_on_http_500() {
        let (url, _guard) = mock_server(500, "").await;
        let client = CapricornClient::new(url);
        let result = client.fetch_by_token("0xDEADBEEF").await;
        assert!(
            result.is_empty(),
            "expected empty vec on HTTP 500, got {} positions",
            result.len()
        );
    }

    // ── success path: raw amount0/amount1 parsing ─────────────────────────────
    //
    // Regression guard for the amountHuman→raw fix (#111). Capricorn returns RAW
    // on-chain amounts in `amount0`/`amount1`; the parser must read those (not the
    // human-scaled `amount0Human`) so V1 lp_balance shares the raw `balance` scale.

    #[tokio::test]
    async fn fetch_by_owner_parses_raw_amount0_amount1() {
        let body = r#"{"data":{"positions":{"positions":[
            {"owner":"0x000000000000000000000000000000000000Aa01",
             "token0":{"tokenAddress":"0x000000000000000000000000000000000000Bb01"},
             "token1":{"tokenAddress":"0x000000000000000000000000000000000000Cc01"},
             "amount0":"1961178955610182134800088","amount1":"0"}
        ]}}}"#;
        let (url, _guard) = mock_server(200, body).await;
        let positions = CapricornClient::new(url)
            .fetch_by_owner("0x000000000000000000000000000000000000Aa01")
            .await;

        assert_eq!(positions.len(), 1, "expected 1 parsed position");
        // raw integer string (wei), NOT a human-scaled decimal
        assert_eq!(positions[0].amount0, "1961178955610182134800088");
        assert_eq!(positions[0].amount1, "0");

        // the raw matching-side amount flows through owner aggregation unchanged
        let agg = lp_amounts_by_owner(&positions, "0x000000000000000000000000000000000000Bb01");
        let owner_cs =
            crate::utils::valid_account_id("0x000000000000000000000000000000000000Aa01").unwrap();
        let found = agg.iter().find(|(o, _)| o == &owner_cs).expect("owner present");
        assert_eq!(
            found.1.normalized().to_plain_string(),
            "1961178955610182134800088"
        );
    }

    // ── fail-to-empty: malformed JSON ─────────────────────────────────────────

    #[tokio::test]
    async fn fetch_by_owner_returns_empty_on_malformed_json() {
        let (url, _guard) = mock_server(200, "not json {{{").await;
        let client = CapricornClient::new(url);
        let result = client.fetch_by_owner("0xABCDEF").await;
        assert!(
            result.is_empty(),
            "expected empty vec on malformed JSON, got {} positions",
            result.len()
        );
    }

    #[tokio::test]
    async fn fetch_by_token_returns_empty_on_malformed_json() {
        let (url, _guard) = mock_server(200, "not json {{{").await;
        let client = CapricornClient::new(url);
        let result = client.fetch_by_token("0xDEADBEEF").await;
        assert!(
            result.is_empty(),
            "expected empty vec on malformed JSON, got {} positions",
            result.len()
        );
    }

    // ── fail-to-empty: GraphQL errors field ──────────────────────────────────

    #[tokio::test]
    async fn fetch_by_owner_returns_empty_on_graphql_errors() {
        let (url, _guard) = mock_server(200, r#"{"errors":[{"message":"boom"}]}"#).await;
        let client = CapricornClient::new(url);
        let result = client.fetch_by_owner("0xABCDEF").await;
        assert!(
            result.is_empty(),
            "expected empty vec on GraphQL errors, got {} positions",
            result.len()
        );
    }

    #[tokio::test]
    async fn fetch_by_token_returns_empty_on_graphql_errors() {
        let (url, _guard) = mock_server(200, r#"{"errors":[{"message":"boom"}]}"#).await;
        let client = CapricornClient::new(url);
        let result = client.fetch_by_token("0xDEADBEEF").await;
        assert!(
            result.is_empty(),
            "expected empty vec on GraphQL errors, got {} positions",
            result.len()
        );
    }

    // ── existing unit tests ───────────────────────────────────────────────────

    #[test]
    fn token_side_amount_matches_lowercase() {
        let p = CapricornPosition {
            owner: "0xAbC".into(),
            token0: "0xToKeN".into(),
            token1: "0xWmon".into(),
            amount0: "12.5".into(),
            amount1: "0".into(),
        };
        assert_eq!(p.amount_for_token("0xTOKEN"), Some("12.5".to_string())); // checksum vs lowercase
        assert_eq!(p.amount_for_token("0xOTHER"), None);
    }

    #[test]
    fn token_side_amount_returns_token1() {
        let p = CapricornPosition {
            owner: "0xAbC".into(),
            token0: "0xAAA".into(),
            token1: "0xBBB".into(),
            amount0: "5.0".into(),
            amount1: "99.9".into(),
        };
        assert_eq!(p.amount_for_token("0xbbb"), Some("99.9".to_string()));
        assert_eq!(p.amount_for_token("0xaaa"), Some("5.0".to_string()));
    }

    #[test]
    fn token_side_amount_none_for_unknown() {
        let p = CapricornPosition {
            owner: "0x1".into(),
            token0: "0x2".into(),
            token1: "0x3".into(),
            amount0: "1".into(),
            amount1: "2".into(),
        };
        assert_eq!(p.amount_for_token("0x4"), None);
    }

    #[test]
    fn lp_amounts_by_token_sums_both_sides_and_checksums() {
        let lower = "0x000000000000000000000000000000000000bb01"; // -> checksum 0x..Bb01
        let positions = vec![
            CapricornPosition {
                owner: "0xa".into(),
                token0: lower.into(),
                token1: "0xwmon".into(),
                amount0: "3".into(),
                amount1: "0".into(),
            },
            CapricornPosition {
                owner: "0xa".into(),
                token0: lower.into(),
                token1: "0xwmon".into(),
                amount0: "4.5".into(),
                amount1: "0".into(),
            },
        ];
        let out = lp_amounts_by_token(&positions);
        let checksum = crate::utils::valid_account_id(lower).unwrap();
        let found = out
            .iter()
            .find(|(id, _)| id == &checksum)
            .expect("token present");
        assert_eq!(found.1.normalized().to_plain_string(), "7.5"); // 3 + 4.5
        // "0xwmon" is not a valid EVM address -> dropped by checksum
        assert!(out.iter().all(|(id, _)| id == &checksum));
    }

    #[test]
    fn lp_amounts_by_owner_sums_per_owner_for_token() {
        let token = "0x000000000000000000000000000000000000bb01";
        let positions = vec![
            CapricornPosition {
                owner: "0x000000000000000000000000000000000000aa01".into(),
                token0: token.into(),
                token1: "0xw".into(),
                amount0: "2".into(),
                amount1: "0".into(),
            },
            CapricornPosition {
                owner: "0x000000000000000000000000000000000000aa01".into(),
                token0: token.into(),
                token1: "0xw".into(),
                amount0: "5".into(),
                amount1: "0".into(),
            },
        ];
        let out = lp_amounts_by_owner(&positions, token);
        let owner_cs =
            crate::utils::valid_account_id("0x000000000000000000000000000000000000aa01").unwrap();
        let found = out
            .iter()
            .find(|(o, _)| o == &owner_cs)
            .expect("owner present");
        assert_eq!(found.1.normalized().to_plain_string(), "7"); // 2 + 5
    }
}
