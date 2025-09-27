#[derive(Debug)]
pub enum MetricsPath {
    Metrics,
}

impl MetricsPath {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetricsPath::Metrics => "/metrics",
        }
    }
    pub fn docs_str(&self) -> &'static str {
        match self {
            MetricsPath::Metrics => "/metrics",
        }
    }
}