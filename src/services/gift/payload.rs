//! Serde types for X Account Activity API webhook payloads. Only the
//! `tweet_create_events` we act on are modeled; unknown fields ignored.
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ActivityPayload {
    #[serde(default)]
    pub tweet_create_events: Vec<TweetCreateEvent>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TweetCreateEvent {
    pub id_str: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub extended_tweet: Option<ExtendedTweet>,
    pub user: TweetUser,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExtendedTweet {
    #[serde(default)]
    pub full_text: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TweetUser {
    pub screen_name: String,
}

impl TweetCreateEvent {
    /// 280+ char tweets truncate `text`; prefer extended_tweet.full_text.
    pub fn resolved_text(&self) -> &str {
        match &self.extended_tweet {
            Some(e) if !e.full_text.is_empty() => &e.full_text,
            _ => &self.text,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_event() {
        let raw = r#"{"tweet_create_events":[{"id_str":"123","text":"hi","user":{"screen_name":"alice"}}]}"#;
        let p: ActivityPayload = serde_json::from_str(raw).unwrap();
        assert_eq!(p.tweet_create_events.len(), 1);
        let e = &p.tweet_create_events[0];
        assert_eq!(e.id_str, "123");
        assert_eq!(e.user.screen_name, "alice");
        assert_eq!(e.resolved_text(), "hi");
    }

    #[test]
    fn extended_text_takes_priority() {
        let raw = r#"{"tweet_create_events":[{"id_str":"1","text":"trunc…","extended_tweet":{"full_text":"full body"},"user":{"screen_name":"bob"}}]}"#;
        let p: ActivityPayload = serde_json::from_str(raw).unwrap();
        assert_eq!(p.tweet_create_events[0].resolved_text(), "full body");
    }

    #[test]
    fn ignores_non_tweet_events() {
        let raw = r#"{"favorite_events":[{"id":"x"}]}"#;
        let p: ActivityPayload = serde_json::from_str(raw).unwrap();
        assert!(p.tweet_create_events.is_empty());
    }
}
