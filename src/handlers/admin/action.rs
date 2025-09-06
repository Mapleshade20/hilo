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

use std::collections::HashSet;
use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

use super::{
    AdminState, build_veto_map, calculate_tag_frequencies, create_final_match, fetch_all_vetoes,
    get_user_id_by_email, get_user_status, is_vetoed,
};
use crate::error::{AppError, AppResult};
use crate::models::{FinalMatch, TagSystem, UserStatus};
use crate::services::matching::MatchingService;

#[derive(Debug, Serialize)]
pub struct TriggerMatchingResponse {
    pub success: bool,
    pub message: &'static str,
    pub matches_created: usize,
    pub matches: Vec<FinalMatch>,
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
/// This endpoint triggers the final matching algorithm which uses a greedy approach
/// to create optimal pairs from users with completed forms. The algorithm respects
/// user vetoes, clears all vetoes and match previews after completion, and updates
/// matched users' status to 'matched'.
///
/// # Returns
///
/// - `200 OK` with `TriggerMatchingResponse` - Final matching completed successfully
/// - `500 Internal Server Error` - Matching algorithm failure
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn trigger_final_matching(
    State(state): State<Arc<AdminState>>,
) -> AppResult<impl IntoResponse> {
    let matches = execute_final_matching(&state.db_pool, state.tag_system)
        .await
        .map_err(|e| {
            error!("Final matching failed: {}", e);
            AppError::Internal
        })?;

    info!("Final matching completed: {} pairs", matches.len());
    Ok((
        StatusCode::OK,
        Json(TriggerMatchingResponse {
            success: true,
            message: "Final matching completed successfully",
            matches_created: matches.len(),
            matches,
        }),
    )
        .into_response())
}

/// Execute the final matching algorithm using greedy approach
async fn execute_final_matching(
    db_pool: &PgPool,
    tag_system: &TagSystem,
) -> AppResult<Vec<FinalMatch>> {
    // Fetch all users with submitted forms
    let forms = MatchingService::fetch_unmatched_forms(db_pool).await?;
    if forms.is_empty() {
        return Ok(vec![]);
    }

    // Fetch all veto records
    let vetoes = fetch_all_vetoes(db_pool).await?;
    let veto_map = build_veto_map(&vetoes);

    // Calculate tag frequencies for IDF scoring
    let tag_frequencies = calculate_tag_frequencies(&forms);
    let total_user_count = forms.len() as u32;

    // Build score matrix for all valid pairs
    let mut pair_scores = Vec::new();

    for (i, form_a) in forms.iter().enumerate() {
        for (j, form_b) in forms.iter().enumerate() {
            if i >= j {
                continue; // Only consider each pair once
            }

            let score = MatchingService::calculate_match_score(
                form_a,
                form_b,
                tag_system,
                &tag_frequencies,
                total_user_count,
            );

            // Apply vetoes - if either user has vetoed the other, set score to -1
            if is_vetoed(form_a.user_id, form_b.user_id, &veto_map)
                || is_vetoed(form_b.user_id, form_a.user_id, &veto_map)
            {
                continue; // Skip vetoed pairs entirely
            }

            if score > 0.0 {
                pair_scores.push((form_a.user_id, form_b.user_id, score));
            }
        }
    }

    // Sort by score (descending) for greedy algorithm
    pair_scores.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    // Greedy matching algorithm
    let mut matched_users = HashSet::new();
    let mut final_matches = Vec::new();

    for (user_a, user_b, score) in pair_scores {
        if !matched_users.contains(&user_a) && !matched_users.contains(&user_b) {
            // Create the final match
            let final_match = create_final_match(db_pool, user_a, user_b, score).await?;
            final_matches.push(final_match);

            matched_users.insert(user_a);
            matched_users.insert(user_b);
        }
    }

    // Update status of matched users to 'matched'
    for final_match in &final_matches {
        sqlx::query!(
            r#"UPDATE users SET status = 'matched' WHERE id = $1"#,
            final_match.user_a_id
        )
        .execute(db_pool)
        .await?;

        sqlx::query!(
            r#"UPDATE users SET status = 'matched' WHERE id = $1"#,
            final_match.user_b_id
        )
        .execute(db_pool)
        .await?;
    }

    // Clear all vetoes and previews after final matching
    info!("Clearing all vetoes and match previews");
    sqlx::query!("DELETE FROM vetoes").execute(db_pool).await?;
    sqlx::query!("DELETE FROM match_previews")
        .execute(db_pool)
        .await?;

    Ok(final_matches)
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
    MatchingService::generate_match_previews(&state.db_pool, state.tag_system)
        .await
        .map_err(|e| {
            error!("Match previews update failed: {}", e);
            AppError::Internal
        })?;

    info!("Match previews update completed successfully");
    Ok((
        StatusCode::OK,
        Json(ActionResponse {
            success: true,
            message: "Match previews updated successfully",
        }),
    )
        .into_response())
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

/// Response for admin user verification
#[derive(Debug, Serialize)]
pub struct VerifyUserResponse {
    pub success: bool,
    pub message: &'static str,
    pub user: UserData,
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

    Ok((
        StatusCode::OK,
        Json(VerifyUserResponse {
            success: true,
            message: "User status updated successfully",
            user: UserData {
                user_id: updated_user.id,
                email: updated_user.email,
                status: updated_user.status,
                grade: updated_user.grade,
                card_photo_filename: updated_user.card_photo_filename,
            },
        }),
    )
        .into_response())
}
