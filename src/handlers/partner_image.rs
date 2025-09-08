//! # Partner Image Serving Handler
//!
//! This module provides protected image serving for partner profile photos.
//! Only users who are matched partners can access each other's images.

use std::path::Path;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Extension, State},
    http::Request,
    response::IntoResponse,
};
use tower_http::services::ServeFile;
use tracing::{debug, error, instrument, trace, warn};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::AppState;
use crate::utils::static_object::UPLOAD_DIR;

/// Serves partner's profile photo for matched users.
///
/// GET /api/images/partner/{filename}
///
/// This endpoint serves profile photos for matched partners only.
/// It requires authentication and validates that the requesting user
/// is actually matched with the user whose photo is being requested.
///
/// # Returns
///
/// - `200 OK` with image file - Partner's profile photo served successfully
/// - `404 Not Found` - User not found or no profile photo available
/// - `500 Internal Server Error` - Database or file system error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn serve_partner_image(
    State(state): State<Arc<AppState>>,
    Extension(requested_id): Extension<Uuid>,
    req: Request<Body>,
) -> AppResult<impl IntoResponse> {
    trace!("Serving partner image");

    // Query the partner's profile photo filename
    let photo_filename = sqlx::query!(
        r#"
        SELECT profile_photo_filename
        FROM forms
        WHERE user_id = $1
        "#,
        requested_id
    )
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| {
        warn!(?requested_id, "User not found in forms table");
        AppError::NotFound("User not found")
    })?
    .profile_photo_filename
    .ok_or_else(|| {
        debug!(?requested_id, "No profile photo found");
        AppError::NotFound("No profile photo found")
    })?;

    // Construct the file path
    let file_path = Path::new(UPLOAD_DIR.as_str())
        .join("profile_photos")
        .join(photo_filename);

    debug!("Serving file: {:?}", file_path);

    // Use tower-http's ServeFile to serve the image
    let mut service = ServeFile::new(file_path);
    service
        .try_call(req)
        .await
        .map(|res| res.into_response())
        .map_err(|e| {
            error!("Failed to serve file: {}", e);
            AppError::Internal
        })
}
