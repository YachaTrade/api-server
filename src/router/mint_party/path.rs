#[derive(Debug)]
pub enum ProfilePath {
    UpdateMintParty,
}

impl ProfilePath {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProfilePath::UpdateMintParty => "/mint_party/update",
        }
    }
}
