#[derive(Debug)]
pub enum ChartPath {
    GetChart,
}

impl ChartPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChartPath::GetChart => "/chart/{token}", // 실제 라우팅에서 사용
        }
    }

    pub fn docs_str(&self) -> &'static str {
        match self {
            ChartPath::GetChart => "/chart/:token", // Swagger docs에서 사용
        }
    }
}
