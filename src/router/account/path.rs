pub enum AccountPath {
    UpdateAccount,
    GetAccount,
}

impl AccountPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountPath::UpdateAccount => "/account/update",
            AccountPath::GetAccount => "/account/get_account",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            AccountPath::UpdateAccount => "/account/update",
            AccountPath::GetAccount => "/account/get_account",
        }
    }
}
