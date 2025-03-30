pub enum BotPath {
    SetMetadata,
}

impl BotPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            BotPath::SetMetadata => "/bot/metadata",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            BotPath::SetMetadata => "/bot/metadata",
        }
    }
}
