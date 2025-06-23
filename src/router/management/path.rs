pub enum ManagementPath {
    DevPosition,
    HoldingTokenManagements,
    AccountLocks,
    AccountWithdrawableLock,
}

impl ManagementPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            ManagementPath::DevPosition => "/management/dev",
            ManagementPath::HoldingTokenManagements => "/management/hold_token",
            ManagementPath::AccountLocks => "/management/lock",
            ManagementPath::AccountWithdrawableLock => "/management/withdraw",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            ManagementPath::DevPosition => "/management/dev",
            ManagementPath::HoldingTokenManagements => "/management/hold_token",
            ManagementPath::AccountLocks => "/management/lock",
            ManagementPath::AccountWithdrawableLock => "/management/withdraw",
        }
    }
}
