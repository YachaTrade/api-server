#[derive(Debug)]
pub enum AuthPath {
    #[doc = "Generate authentication nonce"]
    Nonce,

    #[doc = "Create authentication session"]
    Session,
    #[doc = "Delete authentication session"]
    DeleteSession,
}

impl AuthPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            AuthPath::Nonce => "/auth/nonce",
            AuthPath::Session => "/auth/session",
            AuthPath::DeleteSession => "/auth/delete_session",
        }
    }
}
