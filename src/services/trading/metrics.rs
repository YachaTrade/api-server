use std::sync::Arc;

use crate::{
    controllers::trading::metrics::MetricsController,
    db::postgres::PostgresDatabase,
    result::AppError,
    types::trading::metrics::{MetricsBatchResponse, TimeFrame},
};

pub struct MetricsService {
    postgres: Arc<PostgresDatabase>,
}

impl MetricsService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    pub async fn get_metrics(
        &self,
        token_id: &str,
        timeframes: Vec<TimeFrame>,
    ) -> Result<MetricsBatchResponse, AppError> {
        let controller = MetricsController::new(self.postgres.clone());
        controller
            .trading_metrics_batch(token_id, timeframes)
            .await
            .map_err(|err| AppError::InternalError(err.to_string()))
    }
}
