//! # Final Match Result Handler
//!
//! This module implements endpoints for users to accept or reject their final match results.
//! When a user rejects a match, both users are reverted to 'form_completed' status.

use std::sync::Arc;

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
};
use tracing::{info, instrument, warn};

use crate::error::{AppError, AppResult};
use crate::middleware::AuthUser;
use crate::models::{AppState, UserStatus};

/// Accept a final match result
///
/// POST /api/final-match/accept
///
/// Updates the user's status from 'matched' to 'confirmed'.
/// If both users have accepted, the match is finalized.
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
    sqlx::query!(
        "UPDATE users SET status = 'confirmed' WHERE id = $1",
        user.user_id
    )
    .execute(&state.db_pool)
    .await?;

    info!("User successfully accepted final match");
    Ok(StatusCode::OK.into_response())
}

/// Reject a final match result
///
/// POST /api/final-match/reject
///
/// Reverts both the user and their partner to 'form_completed' status
/// and removes the final match record from the database.
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
    sqlx::query!(
        "UPDATE users SET status = 'form_completed' WHERE id = $1",
        user.user_id
    )
    .execute(tx.as_mut())
    .await?;

    sqlx::query!(
        "UPDATE users SET status = 'form_completed' WHERE id = $1",
        partner_id
    )
    .execute(tx.as_mut())
    .await?;

    sqlx::query!("DELETE FROM final_matches WHERE id = $1", final_match.id)
        .execute(tx.as_mut())
        .await?;

    tx.commit().await?;

    info!(%partner_id, "User successfully rejected final match");
    Ok(StatusCode::OK.into_response())
}
