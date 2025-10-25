//! # Profile Thumbnail Serving Handler
//!
//! This module provides thumbnail image serving for profile photos.
//! Only users with 'form_completed' status can access thumbnails.

use std::{path::Path, sync::Arc};

use axum::{
    body::Body,
    extract::{Extension, Path as AxumPath, Request, State},
    response::IntoResponse,
};
use tower_http::services::ServeFile;
use tracing::{debug, error, instrument, warn};
use uuid::Uuid;

use crate::{
    error::{AppError, AppResult},
    middleware::AuthUser,
    models::{AppState, UserStatus},
    utils::{file::ImageProcessor, static_object::UPLOAD_DIR},
};

/// Serves thumbnail version of a user's profile photo.
///
/// GET /api/images/thumbnail/{user_id}
///
/// This endpoint serves thumbnail versions of profile photos.
/// It requires authentication and 'form_completed' status.
///
/// # Access Control
///
/// - User must be authenticated
/// - User must have 'form_completed' status
/// - Any 'form_completed' user can fetch any other user's thumbnail
///
/// # Returns
///
/// - `200 OK` with image file - Thumbnail served successfully
/// - `403 Forbidden` - User doesn't have 'form_completed' status
/// - `404 Not Found` - User not found or no profile photo available
/// - `500 Internal Server Error` - Database or file system error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4(), target = %user_id, requester = %auth_user.user_id))]
pub async fn serve_profile_thumbnail(
    State(state): State<Arc<AppState>>,
    Extension(auth_user): Extension<AuthUser>,
    AxumPath(user_id): AxumPath<Uuid>,
    req: Request<Body>,
) -> AppResult<impl IntoResponse> {
    // Check if the requester has form_completed status
    let requester_status = UserStatus::query(&state.db_pool, &auth_user.user_id).await?;

    if requester_status != UserStatus::FormCompleted {
        warn!(
            current_status = %requester_status,
            "User doesn't have form_completed status"
        );
        return Err(AppError::Forbidden(
            "Only users with form_completed status can access thumbnails",
        ));
    }

    // Query the target user's profile photo filename
    let photo_filename = sqlx::query!(
        r#"
        SELECT profile_photo_filename
        FROM forms
        WHERE user_id = $1
        "#,
        user_id
    )
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| {
        warn!("User not found in forms table");
        AppError::NotFound("User not found")
    })?
    .profile_photo_filename
    .ok_or_else(|| {
        debug!("No profile photo found");
        AppError::NotFound("No profile photo found")
    })?;

    // Generate thumbnail filename
    let thumbnail_filename = ImageProcessor::thumbnail_filename(&photo_filename);

    // Construct the file path
    let file_path = Path::new(UPLOAD_DIR.as_str())
        .join("profile_photos")
        .join(thumbnail_filename);

    // Use tower-http's ServeFile to serve the thumbnail
    let mut service = ServeFile::new(file_path);
    service
        .try_call(req)
        .await
        .inspect(|res| {
            if res.status().is_success() || res.status().is_redirection() {
                debug!("Thumbnail served successfully");
            } else {
                warn!("Thumbnail serving returned status: {}", res.status());
            }
        })
        .map_err(|e| {
            error!("Failed to serve thumbnail: {}", e);
            AppError::NotFound("Thumbnail not found")
        })
}
