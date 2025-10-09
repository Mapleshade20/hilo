//! # Final Match Result Handler
//!
//! This module implements endpoints for users to accept or reject their final match results.
//! When a user rejects a match, both users are reverted to 'form_completed' status.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, State},
    response::IntoResponse,
};
use tracing::{error, info, instrument, warn};

use crate::{
    error::{AppError, AppResult},
    handlers::get_profile,
    middleware::AuthUser,
    models::{AppState, NextMatchTimeResponse, UserStatus},
    services::scheduler::SchedulerService,
};

/// Accepts a final match result for the authenticated user.
///
/// POST /api/final-match/accept
///
/// Updates the user's status from 'matched' to 'confirmed', indicating
/// acceptance of their final match partner. If both users accept their
/// match, the pairing process is complete. Returns the updated profile.
///
/// # Returns
///
/// - `200 OK` with `ProfileResponse`- Final match accepted successfully
/// - `400 Bad Request` - User is not in 'matched' status
/// - `401 Unauthorized` - Missing or invalid authentication token
/// - `500 Internal Server Error` - Database error
#[instrument(
    skip_all,
    fields(
        user_id = %user.user_id,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn accept_final_match(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> AppResult<impl IntoResponse> {
    // Check user status - must be 'matched'
    let user_status = UserStatus::query(&state.db_pool, &user.user_id).await?;

    if user_status != UserStatus::Matched {
        warn!("User status is {:?}, expected 'matched'", user_status);
        return Err(AppError::BadRequest("User is not in matched status"));
    }

    // Update user status to 'confirmed'
    let result = sqlx::query!(
        "UPDATE users SET status = 'confirmed' WHERE id = $1 AND status = 'matched'",
        user.user_id
    )
    .execute(&state.db_pool)
    .await?;

    if result.rows_affected() == 0 {
        error!("Data race detected while accepting final match");
        return Err(AppError::Internal);
    }
    info!("User accepted final match");

    get_profile(State(state), Extension(user)).await
}

/// Rejects a final match result, reverting both users to 'form_completed' status.
///
/// POST /api/final-match/reject
///
/// Reverts both the user and their partner to 'form_completed' status
/// and removes the final match record from the database. This allows both
/// users to potentially be matched again in future matching rounds. Returns
/// the updated profile.
///
/// # Returns
///
/// - `200 OK` - Final match rejected successfully, both users reverted
/// - `400 Bad Request` - User is not in 'matched' status or no match found
/// - `401 Unauthorized` - Missing or invalid authentication token
/// - `500 Internal Server Error` - Database error
#[instrument(
    skip_all,
    fields(
        user_id = %user.user_id,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn reject_final_match(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> AppResult<impl IntoResponse> {
    // Check user status - must be 'matched'
    let user_status = UserStatus::query(&state.db_pool, &user.user_id).await?;

    if user_status != UserStatus::Matched {
        warn!("User status is {:?}, expected 'matched'", user_status);
        return Err(AppError::BadRequest("User is not in matched status"));
    }

    // Find the partner and final match record
    let final_match = sqlx::query!(
        r#"
        SELECT id, user_a_id, user_b_id
        FROM final_matches
        WHERE user_a_id = $1 OR user_b_id = $1
        "#,
        user.user_id
    )
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| {
        warn!("No final match found for user");
        AppError::BadRequest("No final match found for user")
    })?;

    // Determine partner ID
    let partner_id = if final_match.user_a_id == user.user_id {
        final_match.user_b_id
    } else {
        final_match.user_a_id
    };

    // Begin transaction to atomically revert both users to 'form_completed' status and delete match
    let mut tx = state.db_pool.begin().await?;

    // Update both users' statuses and delete the final match record
    let user_result = sqlx::query!(
        "UPDATE users SET status = 'form_completed' WHERE id = $1 AND status = 'matched'",
        user.user_id
    )
    .execute(tx.as_mut())
    .await?;

    let partner_result = sqlx::query!(
        "UPDATE users SET status = 'form_completed' WHERE id = $1 AND (status = 'matched' OR status = 'confirmed')",
        partner_id
    )
    .execute(tx.as_mut())
    .await?;

    sqlx::query!("DELETE FROM final_matches WHERE id = $1", final_match.id)
        .execute(tx.as_mut())
        .await?;

    if user_result.rows_affected() > 0 && partner_result.rows_affected() > 0 {
        tx.commit().await?;
        info!(%partner_id, "User rejected final match");
    } else {
        tx.rollback().await?;
        error!(final_match_id = %final_match.id, %partner_id, "Data race detected while rejecting final match");
        return Err(AppError::Internal);
    }

    get_profile(State(state), Extension(user)).await
}

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
