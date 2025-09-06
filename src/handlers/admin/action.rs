use std::collections::HashSet;
use std::sync::Arc;

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{error, info, instrument, warn};
use uuid::Uuid;

use super::{
    AdminState, build_veto_map, calculate_tag_frequencies, create_final_match, fetch_all_vetoes,
    get_user_id_by_email, get_user_status, is_vetoed, update_user_status,
};
use crate::models::{FinalMatch, TagSystem, UserStatus};
use crate::services::matching::MatchingService;

#[derive(Debug, Serialize)]
pub struct ActionResponse {
    pub success: bool,
    pub message: &'static str,
}

/// Admin endpoint to trigger the final matching algorithm
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn trigger_final_matching(State(state): State<Arc<AdminState>>) -> impl IntoResponse {
    match execute_final_matching(&state.db_pool, state.tag_system).await {
        Ok(matches) => {
            info!("Final matching completed: {} pairs", matches.len());
            (
                StatusCode::OK,
                Json(ActionResponse {
                    success: true,
                    message: "Final matching completed successfully",
                }),
            )
        }
        Err(e) => {
            error!("Final matching failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ActionResponse {
                    success: false,
                    message: "Final matching failed",
                }),
            )
        }
    }
}

/// Execute the final matching algorithm using greedy approach
async fn execute_final_matching(
    db_pool: &PgPool,
    tag_system: &TagSystem,
) -> Result<Vec<FinalMatch>, Box<dyn std::error::Error + Send + Sync>> {
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

/// Admin endpoint to manually update match previews
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn update_match_previews(State(state): State<Arc<AdminState>>) -> impl IntoResponse {
    match MatchingService::generate_match_previews(&state.db_pool, state.tag_system).await {
        Ok(_) => {
            info!("Match previews update completed successfully");
            (
                StatusCode::OK,
                Json(ActionResponse {
                    success: true,
                    message: "Match previews updated successfully",
                }),
            )
        }
        Err(e) => {
            error!("Match previews update failed: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ActionResponse {
                    success: false,
                    message: "Match previews update failed",
                }),
            )
        }
    }
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
    pub user_id: Uuid,
}

/// Admin endpoint to set user verification status
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn verify_user(
    State(state): State<Arc<AdminState>>,
    Json(payload): Json<VerifyUserRequest>,
) -> impl IntoResponse {
    // Validate target status
    if !matches!(
        payload.status,
        UserStatus::Verified | UserStatus::Unverified
    ) {
        warn!("Invalid target status: {:?}", payload.status);
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyUserResponse {
                success: false,
                message: "Invalid target status",
                user_id: Uuid::nil(),
            }),
        );
    }

    // Get user ID (prioritize user_id over email)
    let user_id = if let Some(id) = payload.user_id {
        id
    } else if let Some(email) = &payload.email {
        match get_user_id_by_email(&state.db_pool, email).await {
            Ok(id) => id,
            Err(code) => {
                warn!("User not found by email: {}", email);
                return (
                    code,
                    Json(VerifyUserResponse {
                        success: false,
                        message: "User not found by email",
                        user_id: Uuid::nil(),
                    }),
                );
            }
        }
    } else {
        warn!("Neither user_id nor email provided");
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyUserResponse {
                success: false,
                message: "Must provide either user_id or email",
                user_id: Uuid::nil(),
            }),
        );
    };

    // Check current user status: should not be 'unverified'
    let current_status = match get_user_status(&state.db_pool, &user_id).await {
        Ok(status) => status,
        Err(code) => {
            error!(%user_id, "Failed to get user status for user_id");
            return (
                code,
                Json(VerifyUserResponse {
                    success: false,
                    message: "Failed to get user status",
                    user_id,
                }),
            );
        }
    };
    if current_status == UserStatus::Unverified {
        warn!(
            %user_id,
            "User status is {:?}, expected verification_pending",
            current_status
        );
        return (
            StatusCode::BAD_REQUEST,
            Json(VerifyUserResponse {
                success: false,
                message: "Cannot change status of an unverified user",
                user_id,
            }),
        );
    }

    // Update user status
    match update_user_status(&state.db_pool, &user_id, payload.status).await {
        Ok(_) => {
            info!(
                %user_id,
                "Successfully updated user status from {:?} to {:?}",
                current_status, payload.status
            );
            (
                StatusCode::OK,
                Json(VerifyUserResponse {
                    success: true,
                    message: "User status updated successfully",
                    user_id,
                }),
            )
        }
        Err(e) => {
            error!("Failed to update user status: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(VerifyUserResponse {
                    success: false,
                    message: "Failed to update user status",
                    user_id,
                }),
            )
        }
    }
}
