#[derive(Debug)]
pub enum XVerificationPath {
    OauthLogin,
    OauthCallback,
    FollowedBy,
    FollowedByDelete,
    Pending,
    Reserve,
    Finalize,
}

impl XVerificationPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            XVerificationPath::OauthLogin => "/x/oauth/login",
            XVerificationPath::OauthCallback => "/x/oauth/callback",
            XVerificationPath::FollowedBy => "/x/followed-by",
            XVerificationPath::FollowedByDelete => "/x/followed-by/:handle",
            XVerificationPath::Pending => "/x/verification/pending",
            XVerificationPath::Reserve => "/x/verification/reserve",
            XVerificationPath::Finalize => "/x/verification/finalize",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            XVerificationPath::OauthLogin => "/x/oauth/login",
            XVerificationPath::OauthCallback => "/x/oauth/callback",
            XVerificationPath::FollowedBy => "/x/followed-by",
            XVerificationPath::FollowedByDelete => "/x/followed-by/{handle}",
            XVerificationPath::Pending => "/x/verification/pending",
            XVerificationPath::Reserve => "/x/verification/reserve",
            XVerificationPath::Finalize => "/x/verification/finalize",
        }
    }
}
