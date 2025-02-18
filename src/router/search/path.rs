pub enum SearchPath {
    Search,
}

impl SearchPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            SearchPath::Search => "/search/:name",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            SearchPath::Search => "/search/{name}",
        }
    }
}
