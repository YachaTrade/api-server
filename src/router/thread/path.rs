pub enum Path {
    CreateThread,
    FixThread,
    RemoveThread,
    LikeThread,
    UnLikeThread,
}

impl Path {
    pub fn as_str(&self) -> &'static str {
        match self {
            Path::CreateThread => "/thread/create",
            Path::FixThread => "/thread/fix",
            Path::RemoveThread => "/thread/remove",
            Path::LikeThread => "/thread/like",
            Path::UnLikeThread => "/thread/unlike",
        }
    }
}
