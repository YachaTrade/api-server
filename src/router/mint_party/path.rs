#[derive(Debug)]
pub enum MintPartyPath {
    UpdateMintParty,
    GetLastJoinMintParty,
    GetMintPartyList,
    GetMintPartyDepositList,
    GetMintPartyBalanceList,
}

impl MintPartyPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            MintPartyPath::UpdateMintParty => "/mint_party/update",
            MintPartyPath::GetLastJoinMintParty => "/mint_party/last_join",
            MintPartyPath::GetMintPartyList => "/mint_party/list",
            MintPartyPath::GetMintPartyDepositList => "/mint_party/deposit_list", //session address check
            MintPartyPath::GetMintPartyBalanceList => "/mint_party/balance", //session address check
        }
    }
    //swager 전용 path
    pub fn docs_str(&self) -> &'static str {
        match self {
            MintPartyPath::UpdateMintParty => "/mint_party/update",
            MintPartyPath::GetLastJoinMintParty => "/mint_party/last_join",
            MintPartyPath::GetMintPartyList => "/mint_party/list",
            MintPartyPath::GetMintPartyDepositList => "/mint_party/deposit_list", //session address check
            MintPartyPath::GetMintPartyBalanceList => "/mint_party/balance", //session address check
        }
    }
}
