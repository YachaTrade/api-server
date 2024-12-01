pub enum Path {
    CreateThread,
    LikeThread,
    UnLikeThread,
}

impl Path {
    pub fn as_str(&self) -> &'static str {
        match self {
            Path::CreateThread => "/thread/create",
            Path::LikeThread => "/thread/like",
            Path::UnLikeThread => "/thread/unlike",
        }
    }
}
