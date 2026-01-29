use std::{collections::HashMap, sync::Arc};

use crate::{
    controllers::hackathon::HackathonController,
    db::postgres::PostgresDatabase,
    types::hackathon::HackathonInfo,
};

pub struct HackathonService {
    postgres: Arc<PostgresDatabase>,
}

impl HackathonService {
    pub fn new(postgres: Arc<PostgresDatabase>) -> Self {
        Self { postgres }
    }

    /// Get hackathon info for a single token
    /// Returns None if not found or on error
    pub async fn get_hackathon_info(&self, token_id: &str) -> Option<HackathonInfo> {
        let controller = HackathonController::new(self.postgres.clone());
        controller.get_hackathon_info(token_id).await
    }

    /// Get hackathon infos for multiple tokens (batch)
    /// Returns HashMap<token_id, HackathonInfo>
    /// Returns empty HashMap on error
    pub async fn get_hackathon_infos_by_token_ids(
        &self,
        token_ids: &[String],
    ) -> HashMap<String, HackathonInfo> {
        let controller = HackathonController::new(self.postgres.clone());
        controller.get_hackathon_infos_by_token_ids(token_ids).await
    }
}
