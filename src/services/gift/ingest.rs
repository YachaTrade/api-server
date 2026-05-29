//! Webhook ingest: ActivityPayload → GiftParser → gift_tweet INSERT.
//! Replaces gift-bot producer's stream loop; parse + insert logic identical.
use sqlx::PgPool;
use tracing::{info, warn};

use crate::services::gift::db::{insert_gift_tweet, token_indexed};
use crate::services::gift::domain::ParsedGift;
use crate::services::gift::parser::GiftParser;
use crate::services::gift::payload::ActivityPayload;

/// Pure parse step (no DB): every tweet_create_event that passes parsing
/// becomes a ParsedGift. Parse rejects are logged at info (noise) and dropped.
pub fn parse_events(parser: &GiftParser, payload: &ActivityPayload) -> Vec<ParsedGift> {
    let mut out = Vec::new();
    for ev in &payload.tweet_create_events {
        match parser.parse(ev.resolved_text(), &ev.user.screen_name, &ev.id_str) {
            Ok(gift) => out.push(gift),
            Err(reject) => info!(tweet_id = %ev.id_str, reason = %reject, "tweet rejected at parse"),
        }
    }
    out
}

/// Parse + persist. Returns count newly inserted (duplicates/parse-rejects excluded).
pub async fn ingest(pool: &PgPool, parser: &GiftParser, payload: &ActivityPayload) -> usize {
    let gifts = parse_events(parser, payload);
    let mut inserted = 0;
    for gift in &gifts {
        // V2 allowlist pre-filter (gift-bot producer parity): skip tokens not in
        // the indexer's `token` table as V2. Optimization only — the consumer's
        // on-chain getGiftInfo is the authoritative gate — so fail OPEN on a DB
        // error: proceed to insert rather than drop a legitimate tweet.
        match token_indexed(pool, gift.token).await {
            Ok(true) => {}
            Ok(false) => {
                warn!(tweet_id = %gift.tweet_id, token = %gift.token, "token not indexed as V2; dropping tweet");
                continue;
            }
            Err(e) => {
                crate::metrics::METRICS.gift.inc_db_read_error();
                warn!(
                    tweet_id = %gift.tweet_id, token = %gift.token, error = %e,
                    "token_indexed lookup failed transiently; proceeding to insert (on-chain check authoritative)"
                );
            }
        }
        match insert_gift_tweet(pool, gift).await {
            Ok(true) => {
                info!(tweet_id = %gift.tweet_id, "gift tweet ingested");
                inserted += 1;
            }
            Ok(false) => info!(tweet_id = %gift.tweet_id, "duplicate gift tweet, skipped"),
            Err(e) => warn!(tweet_id = %gift.tweet_id, error = %e, "gift_tweet insert failed"),
        }
    }
    inserted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::gift::parser::{test_gift_config, GiftParser};
    use crate::services::gift::payload::ActivityPayload;

    fn parser() -> GiftParser {
        GiftParser::new(&test_gift_config()).unwrap()
    }

    #[test]
    fn well_formed_event_parses_to_gift() {
        // Canonical tweet text from parser.rs happy-path tests:
        //   mention_account = "naddotfun"
        //   activation_prefix = "Activating Gift for"
        //   recipient_prefix = "Fees will go to"
        //   required_hashtag = "#Nadfun"
        // Two distinct non-zero addresses: token 0xAAA…A, receiver 0xBBB…B.
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun";
        let raw = format!(
            r#"{{"tweet_create_events":[{{"id_str":"111","text":{},"user":{{"screen_name":"someone"}}}}]}}"#,
            serde_json::to_string(text).unwrap()
        );
        let payload: ActivityPayload = serde_json::from_str(&raw).unwrap();
        let parser = parser();
        let gifts = parse_events(&parser, &payload);
        assert_eq!(gifts.len(), 1);
        assert_eq!(gifts[0].tweet_id, "111");
        assert_eq!(
            format!("{:#x}", gifts[0].token),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(
            format!("{:#x}", gifts[0].receiver),
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
    }

    #[test]
    fn rejected_event_yields_no_gift() {
        let parser = parser();
        let raw = r#"{"tweet_create_events":[{"id_str":"222","text":"just a normal tweet","user":{"screen_name":"x"}}]}"#;
        let payload: ActivityPayload = serde_json::from_str(raw).unwrap();
        assert!(parse_events(&parser, &payload).is_empty());
    }
}
