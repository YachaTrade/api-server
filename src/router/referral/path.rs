pub enum ReferralPath {
    CheckReigsterReferral, //부모 레퍼럴 등록 체크
    RegisterReferral,      //부모 레퍼럴 등록
    MakeReferralCode,      //레퍼럴 코드 생성
    ExistsReferralCode,    //레퍼럴 코드 잇는지 확인
    GetReferralCode,       //레퍼럴 코드 조회
}

impl ReferralPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CheckReigsterReferral => "/referral/register_check",
            Self::RegisterReferral => "/referral/register",
            Self::MakeReferralCode => "/referral/code/make",
            Self::ExistsReferralCode => "/referral/code/exists",
            Self::GetReferralCode => "/referral/code",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            Self::CheckReigsterReferral => "/referral/register_check",
            Self::RegisterReferral => "/referral/register",
            Self::MakeReferralCode => "/referral/code/make",
            Self::ExistsReferralCode => "/referral/code/exists",
            Self::GetReferralCode => "/referral/code",
        }
    }
}
