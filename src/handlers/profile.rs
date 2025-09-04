//! # Profile Handler
//!
//! This module implements the profile endpoint that allows authenticated users
//! to retrieve their own profile information including email and status.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument};

use crate::middleware::AuthUser;
use crate::models::{AppState, FinalPartnerProfile, UserStatus};

/// Response containing user profile information
#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileResponse {
    pub email: String,
    pub status: UserStatus,
    pub grade: Option<String>,
    pub final_match: Option<FinalPartnerProfile>,
}

/// Gets the authenticated user's profile information.
///
/// GET /api/profile
///
/// This endpoint returns the user's email and current status from the database.
/// For users with status 'matched' or 'confirmed', it also returns their partner's
/// profile information. It requires authentication via JWT token and uses the user ID
/// from the token to fetch the most up-to-date profile information.
///
/// # Returns
///
/// - `200 OK` with ProfileResponse - Profile information retrieved successfully
/// - `401 Unauthorized` - Missing or invalid authentication token
/// - `404 Not Found` - User not found in database
/// - `500 Internal Server Error` - Database error
#[instrument(
    skip_all,
    fields(
        user_id = %user.user_id,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> impl IntoResponse {
    debug!("Processing profile request");

    // Query user profile from database
    let result = sqlx::query!(
        r#"SELECT email, status as "status: UserStatus", grade FROM users WHERE id = $1"#,
        user.user_id
    )
    .fetch_optional(&state.db_pool)
    .await;

    match result {
        Ok(Some(row_self)) => {
            // Check if user is matched or confirmed to fetch partner info
            let final_match =
                if matches!(row_self.status, UserStatus::Matched | UserStatus::Confirmed) {
                    fetch_partner_profile(&state, &user.user_id)
                        .await
                        .inspect_err(
                            // rarely happens, only if a revoke happens between requests
                            |e| error!("Failed to fetch partner profile: {}", e),
                        )
                        .ok()
                } else {
                    None
                };

            info!("Profile retrieved successfully");
            (
                StatusCode::OK,
                Json(ProfileResponse {
                    email: row_self.email,
                    status: row_self.status,
                    grade: row_self.grade,
                    final_match,
                }),
            )
                .into_response()
        }
        Ok(None) => {
            error!("User not found in database");
            (StatusCode::NOT_FOUND, "User not found").into_response()
        }
        Err(e) => {
            error!(error = %e, "Database error when fetching profile");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

/// Fetch partner profile information for matched/confirmed users
async fn fetch_partner_profile(
    state: &AppState,
    user_id: &uuid::Uuid,
) -> Result<FinalPartnerProfile, Box<dyn std::error::Error + Send + Sync>> {
    // Find the final match record to get partner ID
    let final_match = sqlx::query!(
        r#"
        SELECT user_a_id, user_b_id
        FROM final_matches
        WHERE user_a_id = $1 OR user_b_id = $1
        "#,
        user_id
    )
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or("No final match found for user")?;

    // Determine partner ID
    let partner_id = if final_match.user_a_id == *user_id {
        final_match.user_b_id
    } else {
        final_match.user_a_id
    };

    // Fetch partner's profile information from users and forms tables
    let partner_info = sqlx::query!(
        r#"
        SELECT
            u.email,
            u.grade,
            f.familiar_tags,
            f.aspirational_tags,
            f.self_intro,
            f.profile_photo_filename
        FROM users u
        LEFT JOIN forms f ON u.id = f.user_id
        WHERE u.id = $1
        "#,
        partner_id
    )
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or("Partner not found")?;

    // Extract email domain
    let email_domain = partner_info
        .email
        .split('@')
        .nth(1)
        .unwrap_or("unknown")
        .to_string();

    // Generate photo URL if partner has a profile photo
    let photo_url = partner_info
        .profile_photo_filename
        .map(|name| format!("/api/images/partner/{name}"));

    Ok(FinalPartnerProfile {
        email_domain,
        grade: partner_info.grade,
        familiar_tags: partner_info.familiar_tags,
        aspirational_tags: partner_info.aspirational_tags,
        self_intro: partner_info.self_intro,
        photo_url,
    })
}
