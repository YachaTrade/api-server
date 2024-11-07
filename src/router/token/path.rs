#[derive(Debug)]
pub enum Path {
    UpdateToken,
}

impl Path {
    pub fn as_str(&self) -> &'static str {
        match self {
            Path::UpdateToken => "/token/update",
        }
    }
}
