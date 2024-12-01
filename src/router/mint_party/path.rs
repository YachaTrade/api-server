#[derive(Debug)]
pub enum MintPartyPath {
    UpdateMintParty,
}

impl MintPartyPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            MintPartyPath::UpdateMintParty => "/mint_party/update",
        }
    }
}
