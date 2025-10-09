//! # Admin Action Handlers
//!
//! This module implements administrative action endpoints that modify application
//! state. These endpoints allow administrators to trigger system operations like
//! final matching, update match previews, and manage user verification status.
//!
//! # Security
//!
//! All endpoints in this module perform write operations and should be protected
//! by appropriate admin authentication middleware in the router configuration.
//!
//! # Operations
//!
//! - **Final Matching** - Executes the matching algorithm to create final pairs
//! - **Match Previews** - Regenerates preview suggestions for all users
//! - **User Verification** - Changes user status for verification workflow

use std::sync::Arc;

use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

use super::{AdminState, get_user_id_by_email, get_user_status};
use crate::error::{AppError, AppResult};
use crate::models::{CreateScheduledMatchesRequest, UserStatus};
use crate::services::{matching::MatchingService, scheduler::SchedulerService};
use crate::utils::static_object::TAG_SYSTEM;

#[derive(Debug, Serialize)]
pub struct TriggerMatchingResponse {
    pub success: bool,
    pub message: &'static str,
    pub matches_created: usize,
}

#[derive(Debug, Serialize)]
pub struct ActionResponse {
    pub success: bool,
    pub message: &'static str,
}

/// Executes the final matching algorithm to create user pairs.
///
/// POST /api/admin/trigger-match
///
/// This endpoint triggers the final matching algorithm and updates matched users'
/// status to 'matched'. All vetoes and match previews are cleared after completion.
///
/// # Returns
///
/// - `200 OK` with `TriggerMatchingResponse` - Final matching completed successfully
/// - `500 Internal Server Error` - Matching algorithm failure
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn trigger_final_matching(
    State(state): State<Arc<AdminState>>,
) -> AppResult<impl IntoResponse> {
    let matches_len = SchedulerService::execute_final_matching(&state.db_pool, &TAG_SYSTEM)
        .await
        .map_err(|e| {
            error!("Final matching failed: {}", e);
            AppError::Internal
        })?;

    info!("Final matching completed: {} pairs", matches_len);
    Ok(Json(TriggerMatchingResponse {
        success: true,
        message: "Final matching completed successfully",
        matches_created: matches_len,
    }))
}

/// Manually regenerates match previews for all eligible users.
///
/// POST /api/admin/update-previews
///
/// This endpoint triggers regeneration of match preview suggestions for users
/// with completed forms. Match previews are used to show potential matches
/// before final matching occurs, allowing users to veto unwanted suggestions.
///
/// # Returns
///
/// - `200 OK` with `ActionResponse` - Match previews updated successfully
/// - `500 Internal Server Error` - Preview generation failure
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn update_match_previews(
    State(state): State<Arc<AdminState>>,
) -> AppResult<impl IntoResponse> {
    MatchingService::generate_match_previews(&state.db_pool, &TAG_SYSTEM)
        .await
        .map_err(|e| {
            error!("Match previews update failed: {}", e);
            AppError::Internal
        })?;

    info!("Match previews update completed successfully");
    Ok(Json(ActionResponse {
        success: true,
        message: "Match previews updated successfully",
    }))
}

/// Request payload for admin user verification
#[derive(Debug, Deserialize)]
pub struct VerifyUserRequest {
    /// User ID (takes priority if both id and email are provided)
    pub user_id: Option<Uuid>,
    /// User email (used if user_id is not provided)
    pub email: Option<String>,
    /// Target verification status (verified or unverified)
    pub status: UserStatus,
}

/// User data structure for verification response
#[derive(Debug, Serialize)]
pub struct UserData {
    pub user_id: Uuid,
    pub email: String,
    pub status: UserStatus,
    pub grade: Option<String>,
    pub card_photo_filename: Option<String>,
}

/// Changes user verification status for admin review workflow.
///
/// POST /api/admin/verify-user VerifyUserRequest
///
/// This endpoint allows administrators to change user status between 'verified'
/// and 'unverified' as part of the student card verification workflow. Users
/// must be in 'verification_pending' status to have their status changed.
///
/// # Returns
///
/// - `200 OK` with `VerifyUserResponse` - User status updated successfully
/// - `400 Bad Request` - Invalid request parameters or user status
/// - `404 Not Found` - User not found
/// - `500 Internal Server Error` - Database error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn verify_user(
    State(state): State<Arc<AdminState>>,
    Json(payload): Json<VerifyUserRequest>,
) -> AppResult<impl IntoResponse> {
    // Validate target status
    if !matches!(
        payload.status,
        UserStatus::Verified | UserStatus::Unverified
    ) {
        warn!("Invalid target status: {:?}", payload.status);
        return Err(AppError::BadRequest("Invalid target status"));
    }

    // Get user ID (prioritize user_id over email)
    let user_id = if let Some(id) = payload.user_id {
        id
    } else if let Some(email) = &payload.email {
        get_user_id_by_email(&state.db_pool, email).await?
    } else {
        warn!("Neither user_id nor email provided");
        return Err(AppError::BadRequest("Must provide either user_id or email"));
    };

    // Check current user status: should not be 'unverified'
    let current_status = get_user_status(&state.db_pool, &user_id).await?;

    if current_status == UserStatus::Unverified {
        warn!(
            %user_id,
            "User status is {:?}, expected verification_pending",
            current_status
        );
        return Err(AppError::BadRequest(
            "Cannot change status of an unverified user",
        ));
    }

    // Update user status and return updated user data
    let updated_user = sqlx::query!(
        r#"UPDATE users SET status = $1 WHERE id = $2
           RETURNING id, email, status as "status: UserStatus", grade, card_photo_filename"#,
        payload.status as UserStatus,
        user_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    info!(
        %user_id,
        "Successfully updated user status from {:?} to {:?}",
        current_status, payload.status
    );

    Ok(Json(UserData {
        user_id: updated_user.id,
        email: updated_user.email,
        status: updated_user.status,
        grade: updated_user.grade,
        card_photo_filename: updated_user.card_photo_filename,
    }))
}

/// Creates multiple scheduled final match triggers.
///
/// POST /api/admin/scheduled-matches CreateScheduledMatchesRequest
///
/// This endpoint allows administrators to schedule automatic final match
/// executions at specified UTC timestamps. The scheduled matches will be
/// executed automatically by the background scheduler service.
///
/// # Returns
///
/// - `201 Created` with `Vec<ScheduledFinalMatch>` - Scheduled matches created successfully
/// - `400 Bad Request` - Invalid timestamps or timestamps in the past
/// - `500 Internal Server Error` - Database error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn create_scheduled_matches(
    State(state): State<Arc<AdminState>>,
    Json(payload): Json<CreateScheduledMatchesRequest>,
) -> AppResult<impl IntoResponse> {
    if payload.scheduled_times.is_empty() {
        return Err(AppError::BadRequest(
            "At least one scheduled time is required",
        ));
    }

    let scheduled_times: Vec<_> = payload
        .scheduled_times
        .into_iter()
        .map(|req| req.scheduled_time)
        .collect();

    let scheduled_matches =
        SchedulerService::create_scheduled_matches(&state.db_pool, &scheduled_times).await?;

    info!(
        "Created {} scheduled final matches",
        scheduled_matches.len()
    );

    Ok((StatusCode::CREATED, Json(scheduled_matches)))
}

/// Gets all scheduled final match triggers.
///
/// GET /api/admin/scheduled-matches
///
/// This endpoint returns all scheduled final match triggers, including
/// pending, completed, and failed ones, ordered by scheduled time.
///
/// # Returns
///
/// - `200 OK` with `Vec<ScheduledFinalMatch>` - All scheduled matches
/// - `500 Internal Server Error` - Database error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_scheduled_matches(
    State(state): State<Arc<AdminState>>,
) -> AppResult<impl IntoResponse> {
    let scheduled_matches = SchedulerService::get_all_scheduled_matches(&state.db_pool).await?;

    Ok(Json(scheduled_matches))
}

/// Cancels a scheduled final match trigger.
///
/// DELETE /api/admin/scheduled-matches/{id}
///
/// This endpoint cancels (deletes) a pending scheduled final match.
/// Only pending matches can be cancelled.
///
/// # Returns
///
/// - `200 OK` with `ActionResponse` - Match cancelled successfully
/// - `404 Not Found` - Match not found or already executed
/// - `500 Internal Server Error` - Database error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4(), match_id = %match_id))]
pub async fn cancel_scheduled_match(
    State(state): State<Arc<AdminState>>,
    AxumPath(match_id): AxumPath<Uuid>,
) -> AppResult<impl IntoResponse> {
    let cancelled = SchedulerService::cancel_scheduled_match(&state.db_pool, match_id).await?;

    if !cancelled {
        return Err(AppError::NotFound(
            "Scheduled match not found or already executed",
        ));
    }

    info!(%match_id, "Cancelled scheduled final match");

    Ok(Json(ActionResponse {
        success: true,
        message: "Scheduled match cancelled successfully",
    }))
}

/// Deletes a final match and reverts both users' status to form_completed.
///
/// DELETE /api/admin/final-matches/{id}
///
/// This endpoint allows administrators to delete a final match by ID and
/// revert both matched users back to 'form_completed' status. This is useful
/// for correcting matching errors or handling user requests to be rematched.
///
/// # Returns
///
/// - `200 OK` with `ActionResponse` - Match deleted and users reverted successfully
/// - `404 Not Found` - Match not found
/// - `500 Internal Server Error` - Database error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4(), match_id = %match_id))]
pub async fn delete_final_match(
    State(state): State<Arc<AdminState>>,
    AxumPath(match_id): AxumPath<Uuid>,
) -> AppResult<impl IntoResponse> {
    // Start a transaction to ensure atomicity
    let mut tx = state.db_pool.begin().await?;

    // Fetch the final match to get user IDs
    let final_match = sqlx::query!(
        r#"SELECT user_a_id, user_b_id FROM final_matches WHERE id = $1"#,
        match_id
    )
    .fetch_optional(tx.as_mut())
    .await?;

    let final_match = match final_match {
        Some(fm) => fm,
        None => {
            tx.rollback().await?;
            warn!(%match_id, "Final match not found");
            return Err(AppError::NotFound("Final match not found"));
        }
    };

    // Delete the final match
    sqlx::query!(r#"DELETE FROM final_matches WHERE id = $1"#, match_id)
        .execute(tx.as_mut())
        .await?;

    // Revert both users' status to form_completed
    sqlx::query!(
        r#"UPDATE users SET status = 'form_completed' WHERE id = $1"#,
        final_match.user_a_id
    )
    .execute(tx.as_mut())
    .await?;

    sqlx::query!(
        r#"UPDATE users SET status = 'form_completed' WHERE id = $1"#,
        final_match.user_b_id
    )
    .execute(tx.as_mut())
    .await?;

    // Commit the transaction
    tx.commit().await?;

    info!(
        %match_id,
        user_a_id = %final_match.user_a_id,
        user_b_id = %final_match.user_b_id,
        "Successfully deleted final match and reverted users to form_completed"
    );

    Ok(Json(ActionResponse {
        success: true,
        message: "Final match deleted and users reverted successfully",
    }))
}
