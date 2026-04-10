#[derive(Debug, Clone, Copy)]
pub enum CmsPath {
    SetNsfw,
    InsertTrend,
    UpdateMetadata,
}

impl CmsPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            CmsPath::SetNsfw => "/cms/token/nsfw",
            CmsPath::InsertTrend => "/cms/trend/insert",
            CmsPath::UpdateMetadata => "/cms/token/metadata",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            CmsPath::SetNsfw => "/cms/token/nsfw",
            CmsPath::InsertTrend => "/cms/trend/insert",
            CmsPath::UpdateMetadata => "/cms/token/metadata",
        }
    }
}
