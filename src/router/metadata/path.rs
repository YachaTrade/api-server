#[derive(Debug)]
pub enum MetadataPath {
    #[doc = "Upload image with NSFW validation"]
    UploadImage,
    #[doc = "Upload metadata to R2 and DB"]
    UploadMetadata,
}

impl MetadataPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetadataPath::UploadImage => "/metadata/image",
            MetadataPath::UploadMetadata => "/metadata/metadata",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            MetadataPath::UploadImage => "/metadata/image",
            MetadataPath::UploadMetadata => "/metadata/metadata",
        }
    }
}
