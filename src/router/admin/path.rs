#[derive(Debug)]
pub enum AdminPath {
    DeletToken,
}

impl AdminPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            AdminPath::DeletToken => "/admin/delete_token",
        }
    }
    pub fn docs_str(&self)-> &'static str {
        match self {
            AdminPath::DeletToken => "/admin/delete_token",
        }
    }
}
