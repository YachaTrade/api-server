#[derive(Debug)]
pub enum MetadataPath {
    #[doc = "Upload image with NSFW validation"]
    UploadImage,
    #[doc = "Upload metadata to R2 and DB"]
    UploadMetadata,
    #[doc = "Get gecko metadata for token"]
    GetGeckoMetadata,
}

impl MetadataPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetadataPath::UploadImage => "/metadata/image",
            MetadataPath::UploadMetadata => "/metadata/metadata",
            MetadataPath::GetGeckoMetadata => "/:chain/:token_address",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            MetadataPath::UploadImage => "/metadata/image",
            MetadataPath::UploadMetadata => "/metadata/metadata",
            MetadataPath::GetGeckoMetadata => "/{chain}/{token_address}",
        }
    }
}
