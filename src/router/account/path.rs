pub enum AccountPath {
    UpdateAccount,
    GetAccount,
    ConnectX,
    DisconnectX,
    GetX,
    RegisterWallet,
    GetWallet,
}

impl AccountPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountPath::UpdateAccount => "/account/update",
            AccountPath::GetAccount => "/account/get_account",
            AccountPath::ConnectX => "/account/connect_x",
            AccountPath::DisconnectX => "/account/disconnect_x",
            AccountPath::GetX => "/account/x",
            AccountPath::RegisterWallet => "/account/register_wallet",
            AccountPath::GetWallet => "/account/wallet",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            AccountPath::UpdateAccount => "/account/update",
            AccountPath::GetAccount => "/account/get_account",
            AccountPath::ConnectX => "/account/connect_x",
            AccountPath::DisconnectX => "/account/disconnect_x",
            AccountPath::GetX => "/account/x",
            AccountPath::RegisterWallet => "/account/register_wallet",
            AccountPath::GetWallet => "/account/wallet",
        }
    }
}
