//! Tweet text → [`ParsedGift`] extraction.
//!
//! Pure text-level checks only (plan §7 steps 1–5):
//! - required hashtag presence (case-insensitive, word-bounded)
//! - target mention presence (case-insensitive, word-bounded)
//! - regex match count == 1 (ambiguous if multiple)
//! - non-zero token / receiver
//! - token != receiver
//!
//! Config-dependent business rules (gift.id ownership, gift.state, receiver
//! vs. GiftVault self) happen in `pipeline::validator`.
//!
//! The three regexes are compiled once at [`GiftParser::new`] from the config
//! template slots; `regex::escape` is applied to every literal so that weird
//! config values cannot inject regex metacharacters (plan §6).

use std::str::FromStr;

use alloy::primitives::Address;
use regex::Regex;
use thiserror::Error;

use crate::services::gift::config::GiftConfig;
use crate::services::gift::domain::ParsedGift;

/// Why a tweet was rejected at parse time.
///
/// Each variant maps to one distinct log / metric bucket — keep them split so
/// operators can tell "noise" (`NoMatch`) from "suspicious" (`Ambiguous`) from
/// "misuse" (`TokenEqualsReceiver`) at a glance.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ParseReject {
    #[error("required hashtag missing from tweet text")]
    MissingHashtag,

    #[error("target mention `@{expected}` missing from tweet text")]
    MissingMention { expected: String },

    #[error("no gift pattern matched — tweet does not contain the template")]
    NoMatch,

    #[error("tweet contains {count} gift patterns — ambiguous, rejected")]
    AmbiguousMultipleMatches { count: usize },

    #[error("token address is the zero address")]
    TokenZeroAddress,

    #[error("receiver address is the zero address")]
    ReceiverZeroAddress,

    #[error("token and receiver are the same address")]
    TokenEqualsReceiver,
}

#[derive(Debug, Error)]
pub enum ParserBuildError {
    #[error("failed to compile regex: {0}")]
    InvalidRegex(#[from] regex::Error),

    #[error("config field `{field}` is empty — parser would match arbitrary text")]
    EmptyField { field: &'static str },
}

pub struct GiftParser {
    gift_regex: Regex,
    hashtag_regex: Regex,
    mention_regex: Regex,
    mention_account: String,
}

impl GiftParser {
    /// Build a parser from configured template slots. Regexes are compiled
    /// once; the resulting `GiftParser` is cheap to share (stateless).
    pub fn new(config: &GiftConfig) -> Result<Self, ParserBuildError> {
        // Defense in depth: `GiftConfig` already rejects empty prefixes,
        // but enforce the same invariant here at the last line of defense.
        // An empty prefix would compile to `\s+(0x<40>)...`, matching arbitrary
        // text — a catastrophic misconfig. Release-mode check, not debug_assert.
        check_non_empty("activation_prefix", &config.activation_prefix)?;
        check_non_empty("recipient_prefix", &config.recipient_prefix)?;
        check_non_empty("required_hashtag", &config.required_hashtag)?;
        check_non_empty("mention_account", &config.mention_account)?;

        // Main extraction: `<activation_prefix> [0x<40>] [\s.,]+ <recipient_prefix> [0x<40>]`.
        // - (?i) — all literal characters case-insensitive.
        // - regex::escape — config values are literals, not patterns.
        // - [\s.,]+ — allow "0xAAA. Fees" / "0xAAA Fees" / "0xAAA,\nFees" naturally.
        // - (?:\[\s*)? and (?:\s*\])? — accept `[0xAAA]`, `[ 0xAAA ]`, or
        //   bare `0xAAA`. Brackets are the new preferred form for the campaign;
        //   bare addresses stay supported as a transition fallback. The
        //   bracket groups are independently optional (`?`), so a half-typed
        //   `[0xAAA` or `0xAAA]` still parses — the captured address is what
        //   matters, and the strict 40-hex match means we can't pull garbage
        //   even from a malformed bracket.
        let gift_pattern = format!(
            r"(?i){prefix1}\s+(?:\[\s*)?(0x[a-fA-F0-9]{{40}})(?:\s*\])?[\s.,]+{prefix2}\s+(?:\[\s*)?(0x[a-fA-F0-9]{{40}})(?:\s*\])?",
            prefix1 = regex::escape(config.activation_prefix.trim()),
            prefix2 = regex::escape(config.recipient_prefix.trim()),
        );

        // Word-bounded hashtag: `#Nadfun` matches but `#NadfunClone` doesn't.
        // `#` is not a word char so `\b` on its right side still fires at the
        // boundary between word char (last of hashtag) and non-word after it.
        let hashtag_pattern = format!(r"(?i){}\b", regex::escape(config.required_hashtag.trim()));

        // Mention: require a non-word char (or start-of-text) immediately before
        // `@` so that email/URL fragments like `foo@naddotfun.com` do NOT satisfy
        // the mention check. Rust regex has no lookbehind, so we use a
        // non-capturing alternation that consumes the leading boundary.
        let mention_pattern = format!(
            r"(?i)(?:^|[^\w]){}\b",
            regex::escape(&format!("@{}", config.mention_account.trim())),
        );

        Ok(Self {
            gift_regex: Regex::new(&gift_pattern)?,
            hashtag_regex: Regex::new(&hashtag_pattern)?,
            mention_regex: Regex::new(&mention_pattern)?,
            mention_account: config.mention_account.trim().to_string(),
        })
    }

    /// Parse a tweet. Returns a `ParsedGift` if every text-level check passes,
    /// else a structured [`ParseReject`] suitable for logging.
    pub fn parse(
        &self,
        text: &str,
        author_username: &str,
        tweet_id: &str,
    ) -> Result<ParsedGift, ParseReject> {
        if !self.hashtag_regex.is_match(text) {
            return Err(ParseReject::MissingHashtag);
        }
        if !self.mention_regex.is_match(text) {
            return Err(ParseReject::MissingMention {
                expected: self.mention_account.clone(),
            });
        }

        let mut captures = self.gift_regex.captures_iter(text);
        let first = match captures.next() {
            None => return Err(ParseReject::NoMatch),
            Some(c) => c,
        };
        // Ambiguous if any further match exists.
        if captures.next().is_some() {
            // +2 because `captures.next()` consumed two already; count the rest.
            let count = 2 + captures.count();
            return Err(ParseReject::AmbiguousMultipleMatches { count });
        }

        // The regex guarantees `0x[a-fA-F0-9]{40}`, so address parsing cannot
        // fail on format. Use lowercase to sidestep EIP-55 checksum mismatch
        // on user-typed addresses (plan §6: "EIP-55 체크섬 또는 lowercase").
        let token_str = first
            .get(1)
            .expect("regex group 1 must exist")
            .as_str()
            .to_ascii_lowercase();
        let receiver_str = first
            .get(2)
            .expect("regex group 2 must exist")
            .as_str()
            .to_ascii_lowercase();
        let token = Address::from_str(&token_str).expect("regex-matched hex parses");
        let receiver = Address::from_str(&receiver_str).expect("regex-matched hex parses");

        if token == Address::ZERO {
            return Err(ParseReject::TokenZeroAddress);
        }
        if receiver == Address::ZERO {
            return Err(ParseReject::ReceiverZeroAddress);
        }
        if token == receiver {
            return Err(ParseReject::TokenEqualsReceiver);
        }

        Ok(ParsedGift {
            tweet_id: tweet_id.to_string(),
            author_username: author_username.to_string(),
            token,
            receiver,
        })
    }
}

fn check_non_empty(field: &'static str, value: &str) -> Result<(), ParserBuildError> {
    if value.trim().is_empty() {
        return Err(ParserBuildError::EmptyField { field });
    }
    Ok(())
}

/// Build a [`GiftConfig`] suitable for parser unit tests.
///
/// Uses the same slot values as the original gift-bot parser tests:
/// - `mention_account` = "naddotfun"
/// - `activation_prefix` = "Activating Gift for"
/// - `recipient_prefix` = "Fees will go to"
/// - `required_hashtag` = "#Nadfun"
///
/// All other required fields are filled with syntactically-valid
/// placeholders. `pub(crate)` so ingest-layer tests can reuse it.
#[cfg(test)]
pub(crate) fn test_gift_config() -> crate::services::gift::config::GiftConfig {
    use std::collections::HashMap;
    let mut m = HashMap::new();
    m.insert("GIFT_X_OAUTH_CONSUMER_KEY".to_string(), "ck_test".to_string());
    m.insert("GIFT_X_OAUTH_CONSUMER_SECRET".to_string(), "cs_test".to_string());
    m.insert("GIFT_X_OAUTH_ACCESS_TOKEN".to_string(), "at_test".to_string());
    m.insert("GIFT_X_OAUTH_ACCESS_TOKEN_SECRET".to_string(), "ats_test".to_string());
    m.insert("GIFT_X_WEBHOOK_ID".to_string(), "wh_test".to_string());
    m.insert("GIFT_X_MENTION_ACCOUNT".to_string(), "naddotfun".to_string());
    m.insert("GIFT_X_ACTIVATION_PREFIX".to_string(), "Activating Gift for".to_string());
    m.insert("GIFT_X_RECIPIENT_PREFIX".to_string(), "Fees will go to".to_string());
    m.insert("GIFT_X_REQUIRED_HASHTAG".to_string(), "#Nadfun".to_string());
    m.insert("GIFT_MAIN_RPC_URL".to_string(), "https://rpc.example/".to_string());
    m.insert("GIFT_MONAD_CHAIN_ID".to_string(), "10143".to_string());
    m.insert(
        "GIFT_VAULT_ADDRESS".to_string(),
        "0x0000000000000000000000000000000000000001".to_string(),
    );
    m.insert(
        "GIFT_BOT_PRIVATE_KEY".to_string(),
        "0x1111111111111111111111111111111111111111111111111111111111111111".to_string(),
    );
    crate::services::gift::config::GiftConfig::from_getter(|k| m.get(k).cloned())
        .expect("valid test config")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn parser() -> GiftParser {
        GiftParser::new(&test_gift_config()).expect("parser compiles")
    }

    // --- Happy path ---

    #[test]
    fn parses_canonical_tweet() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun";
        let gift = parser().parse(text, "alice", "tweet-1").unwrap();
        assert_eq!(gift.tweet_id, "tweet-1");
        assert_eq!(gift.author_username, "alice");
        assert_eq!(
            gift.token.to_string().to_lowercase(),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(
            gift.receiver.to_string().to_lowercase(),
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
    }

    #[test]
    fn case_insensitive_prefixes() {
        for prefix_a in [
            "Activating Gift for",
            "activating gift for",
            "ACTIVATING GIFT FOR",
        ] {
            for prefix_b in ["Fees will go to", "fees will go to", "FEES WILL GO TO"] {
                let text = format!(
                    "{prefix_a} 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                     {prefix_b} 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun"
                );
                let r = parser().parse(&text, "alice", "t");
                assert!(
                    r.is_ok(),
                    "should parse with prefixes `{prefix_a}` / `{prefix_b}` — got {r:?}"
                );
            }
        }
    }

    #[test]
    fn hashtag_case_insensitive() {
        for hashtag in ["#Nadfun", "#NADFUN", "#nadfun"] {
            let text = format!(
                "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                 Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb {hashtag} @naddotfun"
            );
            assert!(
                parser().parse(&text, "alice", "t").is_ok(),
                "failed for {hashtag}"
            );
        }
    }

    #[test]
    fn mention_case_insensitive() {
        for mention in ["@naddotfun", "@Naddotfun", "@NADDOTFUN"] {
            let text = format!(
                "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                 Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun {mention}"
            );
            assert!(
                parser().parse(&text, "alice", "t").is_ok(),
                "failed for {mention}"
            );
        }
    }

    #[test]
    fn accepts_all_lowercase_addresses() {
        let text = "Activating Gift for 0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa. \
                    Fees will go to 0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb #Nadfun @naddotfun";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn accepts_all_uppercase_addresses() {
        let text = "Activating Gift for 0xAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA. \
                    Fees will go to 0xBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB #Nadfun @naddotfun";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn accepts_no_period_between_addresses() {
        // Separator allows spaces/commas/newlines, not just period.
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn accepts_comma_separator() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa, \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn accepts_multiline() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa.\n\
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb\n\
                    #Nadfun @naddotfun";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn mention_hashtag_can_be_anywhere() {
        let text = "@naddotfun #Nadfun\n\
                    Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn extra_text_around_pattern_is_fine() {
        let text = "hey everyone 🎁 Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb — excited!! \
                    #Nadfun @naddotfun 🚀";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    // --- Bracket-wrapped addresses (campaign preferred form) ---

    #[test]
    fn accepts_brackets_around_both_addresses() {
        let text = "Activating Gift for [0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa]. \
                    Fees will go to [0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb] #Nadfun @naddotfun";
        let gift = parser().parse(text, "alice", "t").unwrap();
        assert_eq!(
            gift.token.to_string().to_lowercase(),
            "0xaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(
            gift.receiver.to_string().to_lowercase(),
            "0xbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        );
    }

    #[test]
    fn accepts_brackets_with_internal_whitespace() {
        // User picked "공백 허용 — `[ 0xABC ]` 도 OK".
        let text = "Activating Gift for [ 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa ]. \
                    Fees will go to [\t0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb\t] #Nadfun @naddotfun";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn accepts_mixed_brackets_one_side_only() {
        // Lenient form: brackets on one side, bare on the other. Transition
        // tweets where the campaign updates copy gradually shouldn't break.
        let text = "Activating Gift for [0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa]. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun";
        assert!(parser().parse(text, "alice", "t").is_ok());

        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to [0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb] #Nadfun @naddotfun";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn accepts_brackets_across_newline() {
        let text = "Activating Gift for [0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa]\n\
                    Fees will go to [0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb]\n\
                    #Nadfun @naddotfun";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn brackets_do_not_change_captured_address() {
        // The bracket itself must NOT leak into the captured group. Both
        // forms have to extract the same `Address`.
        let with_brackets = "Activating Gift for [0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa]. \
                             Fees will go to [0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb] #Nadfun @naddotfun";
        let without = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                       Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun";
        let p = parser();
        let a = p.parse(with_brackets, "alice", "t").unwrap();
        let b = p.parse(without, "alice", "t").unwrap();
        assert_eq!(a.token, b.token);
        assert_eq!(a.receiver, b.receiver);
    }

    #[test]
    fn rejects_brackets_with_invalid_hex_inside() {
        // The hex strictness still applies inside brackets. 39 chars must fail.
        let text = "Activating Gift for [0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAA]. \
                    Fees will go to [0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb] #Nadfun @naddotfun";
        assert_eq!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::NoMatch
        );
    }

    // --- Reject path: hashtag ---

    #[test]
    fn rejects_missing_hashtag() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb @naddotfun";
        assert_eq!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::MissingHashtag
        );
    }

    #[test]
    fn hashtag_word_bounded_not_substring() {
        // `#NadfunClone` must NOT satisfy the `#Nadfun` requirement.
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #NadfunClone @naddotfun";
        assert_eq!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::MissingHashtag
        );
    }

    #[test]
    fn hashtag_followed_by_punctuation_still_matches() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun! @naddotfun";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    // --- Reject path: mention ---

    #[test]
    fn rejects_missing_mention() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun";
        assert!(matches!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::MissingMention { .. }
        ));
    }

    #[test]
    fn mention_word_bounded() {
        // `@naddotfunclone` must NOT satisfy the `@naddotfun` requirement.
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfunclone";
        assert!(matches!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::MissingMention { .. }
        ));
    }

    // --- Reject path: pattern ---

    #[test]
    fn rejects_missing_pattern_entirely() {
        let text = "hey @naddotfun check this out #Nadfun 🚀";
        assert_eq!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::NoMatch
        );
    }

    #[test]
    fn rejects_pattern_with_wrong_length_address() {
        // 39-char address — regex requires exactly 40.
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAA. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun";
        assert_eq!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::NoMatch
        );
    }

    #[test]
    fn rejects_ambiguous_multiple_matches() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb. \
                    Or maybe Activating Gift for 0xCCCcccCCCcccCCCcccCCCcccCCCcccCCCcccCCCc. \
                    Fees will go to 0xDDDdddDDDdddDDDdddDDDdddDDDdddDDDdddDDDd. \
                    #Nadfun @naddotfun";
        match parser().parse(text, "alice", "t").unwrap_err() {
            ParseReject::AmbiguousMultipleMatches { count } => assert_eq!(count, 2),
            other => panic!("expected Ambiguous, got {other:?}"),
        }
    }

    // --- Reject path: addresses ---

    #[test]
    fn rejects_zero_token() {
        let text = "Activating Gift for 0x0000000000000000000000000000000000000000. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun";
        assert_eq!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::TokenZeroAddress
        );
    }

    #[test]
    fn rejects_zero_receiver() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0x0000000000000000000000000000000000000000 #Nadfun @naddotfun";
        assert_eq!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::ReceiverZeroAddress
        );
    }

    #[test]
    fn rejects_token_equals_receiver() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa #Nadfun @naddotfun";
        assert_eq!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::TokenEqualsReceiver
        );
    }

    // --- Robustness ---

    #[test]
    fn regex_escape_handles_metachar_config() {
        // Simulate a campaign whose prefix contains regex metachars (e.g. `[DROP]`).
        // Without regex::escape the parser would fail to build or match oddly.
        let mut m: HashMap<String, String> = HashMap::new();
        m.insert("GIFT_X_OAUTH_CONSUMER_KEY".to_string(), "ck_test".to_string());
        m.insert("GIFT_X_OAUTH_CONSUMER_SECRET".to_string(), "cs_test".to_string());
        m.insert("GIFT_X_OAUTH_ACCESS_TOKEN".to_string(), "at_test".to_string());
        m.insert("GIFT_X_OAUTH_ACCESS_TOKEN_SECRET".to_string(), "ats_test".to_string());
        m.insert("GIFT_X_WEBHOOK_ID".to_string(), "wh_test".to_string());
        m.insert("GIFT_X_MENTION_ACCOUNT".to_string(), "naddotfun".to_string());
        m.insert("GIFT_X_ACTIVATION_PREFIX".to_string(), "[PROMO] Gift for".to_string());
        m.insert("GIFT_X_RECIPIENT_PREFIX".to_string(), "(Fees) to".to_string());
        m.insert("GIFT_X_REQUIRED_HASHTAG".to_string(), "#Nadfun".to_string());
        m.insert("GIFT_MAIN_RPC_URL".to_string(), "https://rpc.example/".to_string());
        m.insert("GIFT_MONAD_CHAIN_ID".to_string(), "10143".to_string());
        m.insert(
            "GIFT_VAULT_ADDRESS".to_string(),
            "0x0000000000000000000000000000000000000001".to_string(),
        );
        m.insert(
            "GIFT_BOT_PRIVATE_KEY".to_string(),
            "0x1111111111111111111111111111111111111111111111111111111111111111".to_string(),
        );
        let cfg = crate::services::gift::config::GiftConfig::from_getter(|k| m.get(k).cloned())
            .expect("config valid");
        let parser = GiftParser::new(&cfg).expect("parser compiles with meta-char prefixes");

        let text = "[PROMO] Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    (Fees) to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun";
        assert!(parser.parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn parser_preserves_author_handle_case() {
        // Parser doesn't lowercase the handle — validator handles case-insensitive
        // match against gift.id. Verify the ParsedGift echoes the original.
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun";
        let gift = parser().parse(text, "AliceCaseMix", "t").unwrap();
        assert_eq!(gift.author_username, "AliceCaseMix");
    }

    #[test]
    fn parser_passes_through_non_ascii_author_handle() {
        // X handles are ASCII-only on X's side, but the parser itself does no
        // validation on `author_username` — it's treated as an opaque string.
        // Pin the contract explicitly.
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun @naddotfun";
        let gift = parser().parse(text, "앨리스🐰", "t").unwrap();
        assert_eq!(gift.author_username, "앨리스🐰");
    }

    // --- Review round 1: mention boundary (H1) ---

    #[test]
    fn mention_rejects_email_embedded() {
        // `foo@naddotfun.com` must NOT satisfy the `@naddotfun` mention check.
        // Before the leading-non-word guard this slipped through via \b after
        // the account name — exactly the kind of email/URL fragment we want
        // our double-defence to stop.
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun \
                    contact: support@naddotfun.com";
        assert!(matches!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::MissingMention { .. }
        ));
    }

    #[test]
    fn mention_rejects_url_fragment() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun \
                    see docs: https://docs@naddotfun.io";
        assert!(matches!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::MissingMention { .. }
        ));
    }

    #[test]
    fn mention_accepts_start_of_text() {
        // The leading-boundary rule must treat start-of-string as a valid
        // boundary — not require an actual character before `@`.
        let text = "@naddotfun Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn mention_accepts_inside_parens() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun (@naddotfun)";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn mention_accepts_after_newline() {
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun\n\
                    @naddotfun";
        assert!(parser().parse(text, "alice", "t").is_ok());
    }

    #[test]
    fn mention_rejects_preceded_by_unicode_word_char() {
        // `[^\w]` is Unicode-aware in the regex crate, so any non-ASCII word
        // character immediately before `@` (Hangul, CJK, Cyrillic, Arabic, etc.)
        // disqualifies the mention — same contract as ASCII letters. Users must
        // separate the mention with whitespace/punctuation. Pin this intended
        // behavior so a future swap to a custom boundary class doesn't drift.
        let text = "Activating Gift for 0xAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAaaaAAAa. \
                    Fees will go to 0xBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBbbbBBBb #Nadfun \
                    받으세요@naddotfun";
        assert!(matches!(
            parser().parse(text, "alice", "t").unwrap_err(),
            ParseReject::MissingMention { .. }
        ));
    }

    // --- Review round 2: empty-field runtime check ---

    #[test]
    fn parser_build_rejects_empty_prefix() {
        // Even if GiftConfig is bypassed (e.g. a caller builds GiftConfig
        // directly), GiftParser::new must refuse to compile an empty-prefix regex
        // that would otherwise match arbitrary text.
        let mut cfg = test_gift_config();
        cfg.activation_prefix = "   ".to_string();
        match GiftParser::new(&cfg) {
            Err(ParserBuildError::EmptyField { field }) => {
                assert_eq!(field, "activation_prefix")
            }
            Err(other) => panic!("expected EmptyField, got {other:?}"),
            Ok(_) => panic!("expected EmptyField, got Ok"),
        }
    }

    #[test]
    fn parser_build_rejects_empty_mention_account() {
        let mut cfg = test_gift_config();
        cfg.mention_account = "".to_string();
        match GiftParser::new(&cfg) {
            Err(ParserBuildError::EmptyField { field }) => {
                assert_eq!(field, "mention_account")
            }
            Err(other) => panic!("expected EmptyField, got {other:?}"),
            Ok(_) => panic!("expected EmptyField, got Ok"),
        }
    }
}
