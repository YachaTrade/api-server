pub enum Path {
    UpdateAccount,
    AddAccountLike,
    RemoveAccountLike,
    GetAccount,
}

impl Path {
    pub fn as_str(&self) -> &'static str {
        match self {
            Path::UpdateAccount => "/account/update",
            Path::AddAccountLike => "/account/like",
            Path::RemoveAccountLike => "/account/unlike",
            Path::GetAccount => "/account/get_account",
        }
    }
}
