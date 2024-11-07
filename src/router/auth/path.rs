#[derive(Debug)]
pub enum Path {
    #[doc = "Generate authentication nonce"]
    Nonce,

    #[doc = "Create authentication session"]
    Session,
    #[doc = "Delete authentication session"]
    DeleteSession,
}

impl Path {
    pub fn as_str(&self) -> &'static str {
        match self {
            Path::Nonce => "/auth/nonce",
            Path::Session => "/auth/session",
            Path::DeleteSession => "/auth/delete_session",
        }
    }
}
