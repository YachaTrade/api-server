use axum::{extract::State, response::Json};

use crate::{
    result::AppJsonResult, router::new_event::path::NewEventPath,
    services::new_event::NewEventService, state::AppState, types::new_event::NewEventResponse,
};

/// Get latest new events (buy/sell/create)
#[utoipa::path(
    get,
    path = NewEventPath::NewEvent.docs_str(),
    responses(
        (status = 200, description = "Latest new events retrieved successfully", body = NewEventResponse),
        (status = 500, description = "Internal server error")
    ),
    tag = "New Event"
)]
pub async fn get_new_event(State(state): State<AppState>) -> AppJsonResult<NewEventResponse> {
    let service = NewEventService::new(state.postgres.clone(), state.redis.clone());
    let response = service.get_new_events().await?;
    Ok(Json(response))
}
