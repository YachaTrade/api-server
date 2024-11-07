pub enum Path {
    AddFollow,
    RemoveFollow,
}

impl Path {
    pub fn as_str(&self) -> &'static str {
        match self {
            Path::AddFollow => "/follow/add",
            Path::RemoveFollow => "/follow/remove",
        }
    }
}
