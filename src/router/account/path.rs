pub enum AccountPath {
    UpdateAccount,
    GetAccount,
    UploadImage,
    RegisterWallet,
    GetWallet,
}

impl AccountPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountPath::UpdateAccount => "/account/update",
            AccountPath::GetAccount => "/account/get_account",
            AccountPath::UploadImage => "/account/image",
            AccountPath::RegisterWallet => "/account/register_wallet",
            AccountPath::GetWallet => "/account/wallet",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            AccountPath::UpdateAccount => "/account/update",
            AccountPath::GetAccount => "/account/get_account",
            AccountPath::UploadImage => "/account/image",
            AccountPath::RegisterWallet => "/account/register_wallet",
            AccountPath::GetWallet => "/account/wallet",
        }
    }
}
