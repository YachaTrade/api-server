pub enum NewContentPath {
    NewContent,
}

impl NewContentPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            NewContentPath::NewContent => "/new_content",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            NewContentPath::NewContent => "/new_content",
        }
    }
}
