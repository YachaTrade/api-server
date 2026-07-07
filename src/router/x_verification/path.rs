#[derive(Debug)]
pub enum XVerificationPath {
    OauthLogin,
    OauthLogout,
    OauthCallback,
    FollowedBy,
    FollowedByDelete,
    Status,
    Reserve,
    Finalize,
}

impl XVerificationPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            XVerificationPath::OauthLogin => "/x/oauth/login",
            XVerificationPath::OauthLogout => "/x/oauth/logout",
            XVerificationPath::OauthCallback => "/x/oauth/callback",
            XVerificationPath::FollowedBy => "/x/followed-by",
            XVerificationPath::FollowedByDelete => "/x/followed-by/:handle",
            XVerificationPath::Status => "/x/verification/status",
            XVerificationPath::Reserve => "/x/verification/reserve",
            XVerificationPath::Finalize => "/x/verification/finalize",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            XVerificationPath::OauthLogin => "/x/oauth/login",
            XVerificationPath::OauthLogout => "/x/oauth/logout",
            XVerificationPath::OauthCallback => "/x/oauth/callback",
            XVerificationPath::FollowedBy => "/x/followed-by",
            XVerificationPath::FollowedByDelete => "/x/followed-by/{handle}",
            XVerificationPath::Status => "/x/verification/status",
            XVerificationPath::Reserve => "/x/verification/reserve",
            XVerificationPath::Finalize => "/x/verification/finalize",
        }
    }
}
