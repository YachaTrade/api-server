#[derive(Debug, Clone, Copy)]
pub enum CmsPath {
    SetNsfw,
    InsertTrend,
    UpdateMetadata,
    UploadDexTokenImage,
    UpsertWhitelistToken,
    ListWhitelistToken,
    DeleteDevPost,
    RestoreDevPost,
}

impl CmsPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            CmsPath::SetNsfw => "/cms/token/nsfw",
            CmsPath::InsertTrend => "/cms/trend/insert",
            CmsPath::UpdateMetadata => "/cms/token/metadata",
            CmsPath::UploadDexTokenImage => "/cms/dex-token/image",
            CmsPath::UpsertWhitelistToken => "/cms/whitelist-token",
            CmsPath::ListWhitelistToken => "/cms/whitelist-token",
            CmsPath::DeleteDevPost => "/cms/dev-post/:post_id",
            CmsPath::RestoreDevPost => "/cms/dev-post/:post_id/restore",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            CmsPath::DeleteDevPost => "/cms/dev-post/{post_id}",
            CmsPath::RestoreDevPost => "/cms/dev-post/{post_id}/restore",
            _ => self.as_str(),
        }
    }
}
