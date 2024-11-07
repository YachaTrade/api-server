#[derive(Debug)]
pub enum Path {
    GetBalance,
}

impl Path {
    pub fn as_str(&self) -> &'static str {
        match self {
            Path::GetBalance => "/balance/:account_address",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            Path::GetBalance => "/balance/{account_address}", // Swagger 문서용
        }
    }
}
