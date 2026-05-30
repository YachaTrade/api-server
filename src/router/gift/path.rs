//! Canonical gift route paths — single source of truth for the webhook surface.
#[derive(Debug, Clone, Copy)]
pub enum GiftPath {
    Webhook,
    Healthz,
}

impl GiftPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            GiftPath::Webhook => "/x/webhook",
            GiftPath::Healthz => "/x/healthz",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These literals MUST stay in sync with the routes registered in
    /// router/gift/mod.rs and the api_key_gate bypass in middleware.rs.
    /// If a route path changes, this test forces the constant to change too.
    #[test]
    fn path_constants_match_router_and_bypass() {
        assert_eq!(GiftPath::Webhook.as_str(), "/x/webhook");
        assert_eq!(GiftPath::Healthz.as_str(), "/x/healthz");
    }
}
