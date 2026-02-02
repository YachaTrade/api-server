#[derive(Debug, Clone, Copy)]
pub enum ApiKeyPath {
    ApiKey,
    ApiKeyId,
}

impl ApiKeyPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            ApiKeyPath::ApiKey => "/api-key",
            ApiKeyPath::ApiKeyId => "/api-key/:id",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            ApiKeyPath::ApiKey => "/api-key",
            ApiKeyPath::ApiKeyId => "/api-key/{id}",
        }
    }
}
