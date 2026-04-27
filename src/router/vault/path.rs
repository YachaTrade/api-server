#[derive(Debug)]
pub enum VaultPath {
    GetTokenVaults,
}

impl VaultPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            VaultPath::GetTokenVaults => "/vault/:token_address",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            VaultPath::GetTokenVaults => "/vault/{token_address}",
        }
    }
}
