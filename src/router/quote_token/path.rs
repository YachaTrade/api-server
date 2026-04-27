#[derive(Debug)]
pub enum QuoteTokenPath {
    List,
}

impl QuoteTokenPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            QuoteTokenPath::List => "/quote_token",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            QuoteTokenPath::List => "/quote_token",
        }
    }
}
