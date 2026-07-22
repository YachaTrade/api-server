#[derive(Debug, Clone, Copy)]
pub enum AnalyticsPath {
    ChurnedUsers,
    ActiveUsers,
    NewUsers,
    UserRoi,
}

impl AnalyticsPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            AnalyticsPath::ChurnedUsers => "/cms/analytics/churned-users",
            AnalyticsPath::ActiveUsers => "/cms/analytics/active-users",
            AnalyticsPath::NewUsers => "/cms/analytics/new-users",
            AnalyticsPath::UserRoi => "/cms/analytics/user-roi",
        }
    }

    pub fn docs_str(&self) -> &'static str {
        self.as_str()
    }
}
