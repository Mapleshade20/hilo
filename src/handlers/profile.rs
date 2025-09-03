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
use crate::models::{AppState, UserStatus};

/// Response containing user profile information
#[derive(Debug, Serialize, Deserialize)]
pub struct ProfileResponse {
    pub email: String,
    pub status: UserStatus,
    pub grade: Option<String>,
}

/// Gets the authenticated user's profile information.
///
/// GET /api/profile
///
/// This endpoint returns the user's email and current status from the database.
/// It requires authentication via JWT token and uses the user ID from the token
/// to fetch the most up-to-date profile information.
///
/// # Returns
///
/// - `200 OK` - Profile information retrieved successfully
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
        Ok(Some(row)) => {
            info!("Profile retrieved successfully");
            (
                StatusCode::OK,
                Json(ProfileResponse {
                    email: row.email,
                    status: row.status,
                    grade: row.grade,
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
