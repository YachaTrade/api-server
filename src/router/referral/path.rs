pub enum ReferralPath {
    CheckReferral,
    RegisterReferral,
    MakeReferral,
}

impl ReferralPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CheckReferral => "/referral/check",
            Self::RegisterReferral => "/referral/register",
            Self::MakeReferral => "/referral/make",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            Self::CheckReferral => "/referral/check",
            Self::RegisterReferral => "/referral/register",
            Self::MakeReferral => "/referral/make",
        }
    }
}
