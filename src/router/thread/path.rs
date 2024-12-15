pub enum Path {
    CreateThread,
    LikeThread,
    UnLikeThread,
    GetThreadLike,
}

impl Path {
    pub fn as_str(&self) -> &'static str {
        match self {
            Path::CreateThread => "/thread/create",
            Path::LikeThread => "/thread/like",
            Path::UnLikeThread => "/thread/unlike",
            Path::GetThreadLike => "/thread/like/:token_id",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            Path::CreateThread => "/thread/create",
            Path::LikeThread => "/thread/like",
            Path::UnLikeThread => "/thread/unlike",
            Path::GetThreadLike => "/thread/like/{token_id}",
        }
    }
}
