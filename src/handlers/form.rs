//! # Form Handler
//!
//! This module implements form endpoints that allow verified users to submit
//! and retrieve their form data including tags, traits, and profile information.

use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::middleware::AuthUser;
use crate::models::{AppState, Form, Gender, UserStatus};
use crate::utils::upload;

#[derive(Debug, Serialize, Deserialize)]
pub struct FormRequest {
    pub wechat_id: String,
    pub gender: Gender,
    pub familiar_tags: Vec<String>,
    pub aspirational_tags: Vec<String>,
    pub recent_topics: String,
    pub self_traits: Vec<String>,
    pub ideal_traits: Vec<String>,
    pub physical_boundary: i16,
    pub self_intro: String,
    pub profile_photo_path: Option<String>,
}

/// Submits or updates the authenticated user's form data.
///
/// POST /api/form FormRequest
///
/// This endpoint validates the form data including tag limits and profile photo paths,
/// then saves or updates the user's form in the database. Only users with 'verified'
/// or 'form_completed' status can access this endpoint.
///
/// # Returns
///
/// - `200 OK` - Form submitted/updated successfully
/// - `400 Bad Request` - Invalid form data or validation errors
/// - `401 Unauthorized` - Missing or invalid authentication token
/// - `403 Forbidden` - User status doesn't allow form submission
/// - `500 Internal Server Error` - Database error
#[instrument(
    skip_all,
    fields(
        user_id = %user.user_id,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn submit_form(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(payload): Json<FormRequest>,
) -> impl IntoResponse {
    debug!("Processing form submission request");

    // 1. Check user status - only verified and form_completed users can submit
    let user_status = match UserStatus::query(&state.db_pool, &user.user_id).await {
        Ok(status) => status,
        Err(resp) => {
            error!("Failed to query user status from database");
            return resp.into_response();
        }
    };

    if !user_status.can_fill_form() {
        warn!(current_status = %user_status, "User status doesn't allow form submission");
        return (
            StatusCode::FORBIDDEN,
            "User status doesn't allow form submission",
        )
            .into_response();
    }

    // 2. Validate each field of the form
    if let Some(err_resp) = payload.validate_request(state.tag_system) {
        return err_resp;
    }

    // 3. Validate profile photo path if provided
    if let Some(ref photo_path) = payload.profile_photo_path
        && let Err(resp) = validate_profile_photo_path(photo_path, &user.user_id).await
    {
        return resp.into_response();
    }

    // 4. Insert or update form data
    let result = sqlx::query!(
        r#"
        INSERT INTO forms (user_id, gender, familiar_tags, aspirational_tags, recent_topics,
                          self_traits, ideal_traits, physical_boundary, self_intro, profile_photo_path)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (user_id)
        DO UPDATE SET
            gender = EXCLUDED.gender,
            familiar_tags = EXCLUDED.familiar_tags,
            aspirational_tags = EXCLUDED.aspirational_tags,
            recent_topics = EXCLUDED.recent_topics,
            self_traits = EXCLUDED.self_traits,
            ideal_traits = EXCLUDED.ideal_traits,
            physical_boundary = EXCLUDED.physical_boundary,
            self_intro = EXCLUDED.self_intro,
            profile_photo_path = EXCLUDED.profile_photo_path
        "#,
        user.user_id,
        payload.gender as Gender,
        &payload.familiar_tags,
        &payload.aspirational_tags,
        payload.recent_topics,
        &payload.self_traits,
        &payload.ideal_traits,
        payload.physical_boundary,
        payload.self_intro,
        payload.profile_photo_path
    )
    .execute(&state.db_pool)
    .await;

    match result {
        Ok(_) => {
            // Update wechat_id in users table
            let wechat_update_result = sqlx::query!(
                "UPDATE users SET wechat_id = $1 WHERE id = $2",
                payload.wechat_id,
                user.user_id
            )
            .execute(&state.db_pool)
            .await;

            if let Err(e) = wechat_update_result {
                error!(error = %e, "Failed to update wechat_id in users table");
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to update wechat_id",
                )
                    .into_response();
            }

            // Update user status to form_completed if currently verified
            if user_status == UserStatus::Verified {
                let update_result = sqlx::query!(
                    "UPDATE users SET status = 'form_completed' WHERE id = $1",
                    user.user_id
                )
                .execute(&state.db_pool)
                .await;

                if let Err(e) = update_result {
                    error!(error = %e, "Failed to update user status to form_completed");
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed to update user status",
                    )
                        .into_response();
                }

                info!("User status updated to form_completed");
            }

            info!("Form submitted successfully");
            (StatusCode::OK, "Form submitted successfully").into_response()
        }
        Err(e) => {
            error!(error = %e, "Database error when submitting form");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

/// Retrieves the authenticated user's form data.
///
/// GET /api/form
///
/// This endpoint returns the user's submitted form data. Only users with 'verified'
/// or 'form_completed' status can access this endpoint.
///
/// # Returns
///
/// - `200 OK` - Form data retrieved successfully
/// - `401 Unauthorized` - Missing or invalid authentication token
/// - `404 Not Found` - User has not submitted a form yet
/// - `500 Internal Server Error` - Database error
#[instrument(
    skip_all,
    fields(
        user_id = %user.user_id,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn get_form(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> impl IntoResponse {
    debug!("Processing get form request");

    let result = sqlx::query_as!(
        Form,
        r#"
        SELECT user_id, gender as "gender: Gender", familiar_tags, aspirational_tags,
               recent_topics, self_traits, ideal_traits, physical_boundary,
               self_intro, profile_photo_path
        FROM forms
        WHERE user_id = $1
        "#,
        user.user_id
    )
    .fetch_optional(&state.db_pool)
    .await;

    match result {
        Ok(Some(form)) => {
            info!("Form retrieved successfully");
            (StatusCode::OK, Json(form)).into_response()
        }
        Ok(None) => {
            info!("User has not submitted a form yet");
            (StatusCode::NOT_FOUND, "Form not found").into_response()
        }
        Err(e) => {
            error!(error = %e, "Database error when fetching form");
            (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response()
        }
    }
}

async fn validate_profile_photo_path(
    full_path: &str,
    user_id: &Uuid,
) -> Result<(), impl IntoResponse> {
    let photo_uuid = match upload::FileManager::parse_uuid_from_path(full_path) {
        Some(uuid) => uuid,
        None => {
            warn!("Invalid profile photo path format: {}", full_path);
            return Err((StatusCode::BAD_REQUEST, "Invalid profile photo path"));
        }
    };

    if photo_uuid != *user_id {
        warn!(
            "Photo UUID {} doesn't match user ID {}",
            photo_uuid, user_id
        );
        return Err((StatusCode::BAD_REQUEST, "Photo UUID doesn't match user ID"));
    }

    if !fs::try_exists(full_path).await.unwrap_or(false) {
        warn!("Profile photo file doesn't exist: {:?}", full_path);
        return Err((StatusCode::BAD_REQUEST, "Profile photo file doesn't exist"));
    }

    Ok(())
}
