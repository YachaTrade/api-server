use std::sync::Arc;

use tracing::warn;

use crate::{
    controllers::search::SearchController,
    db::{postgres::PostgresDatabase, redis::RedisDatabase},
    result::AppError,
    types::{common::pagination::PaginationParams, search::SearchResponse},
};

pub struct SearchService {
    postgres: Arc<PostgresDatabase>,
    redis: Arc<RedisDatabase>,
}

impl SearchService {
    pub fn new(postgres: Arc<PostgresDatabase>, redis: Arc<RedisDatabase>) -> Self {
        Self { postgres, redis }
    }

    pub async fn search(
        &self,
        query: &str,
        pagination: PaginationParams,
    ) -> Result<SearchResponse, AppError> {
        if let Ok(Some(cached_response)) = self.redis.get_search_response(query, pagination).await {
            return Ok(cached_response);
        }

        let controller = SearchController::new(self.postgres.clone());
        let response = controller.search(query).await.map_err(|err| {
            AppError::InternalError(format!("Failed to search: {}, error: {}", query, err))
        })?;

        if let Err(err) = self.redis.set_search_response(query, &response).await {
            warn!("Failed to cache search response: {}", err);
        }

        Ok(response)
    }
}
