pub enum DevPostPath {
    Trending,
    Ranking,
    UploadImage,
    Feed,
    Create,
    Detail,
    Like,
    Vote,
}

impl DevPostPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            DevPostPath::Trending => "/dev-post/trending",
            DevPostPath::Ranking => "/dev-post/ranking",
            DevPostPath::UploadImage => "/dev-post/image",
            DevPostPath::Feed | DevPostPath::Create => "/dev-post",
            DevPostPath::Detail => "/dev-post/{post_id}",
            DevPostPath::Like => "/dev-post/{post_id}/like",
            DevPostPath::Vote => "/dev-post/{post_id}/vote",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            DevPostPath::Trending => "/dev-post/trending",
            DevPostPath::Ranking => "/dev-post/ranking",
            DevPostPath::UploadImage => "/dev-post/image",
            DevPostPath::Feed | DevPostPath::Create => "/dev-post",
            DevPostPath::Detail => "/dev-post/{post_id}",
            DevPostPath::Like => "/dev-post/{post_id}/like",
            DevPostPath::Vote => "/dev-post/{post_id}/vote",
        }
    }
}
