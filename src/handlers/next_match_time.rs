use std::sync::Arc;

use axum::{Json, extract::State, response::IntoResponse};
use tracing::instrument;

use crate::error::AppResult;
use crate::models::{AppState, NextMatchTimeResponse};
use crate::services::scheduler::SchedulerService;

/// Gets the next scheduled final match time for users.
///
/// GET /api/next-match-time
///
/// This endpoint returns the earliest scheduled final match time that is
/// still pending and in the future. Users can use this to know when the
/// next automatic matching will occur. Returns null if no matches are scheduled.
///
/// # Returns
///
/// - `200 OK` with `NextMatchTimeResponse` - Next match time or null if none (if next_match_time
/// is earlier than current time, this means the last scheduled match is being processed)
/// - `500 Internal Server Error` - Database error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_next_match_time(
    State(state): State<Arc<AppState>>,
) -> AppResult<impl IntoResponse> {
    let next = SchedulerService::get_next_scheduled_time(&state.db_pool).await?;

    Ok(Json(NextMatchTimeResponse { next }))
}
