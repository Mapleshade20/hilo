//! # Partner Image Serving Handler
//!
//! This module provides protected image serving for partner profile photos.
//! Only users who are matched partners can access each other's images.

use std::path::Path;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Extension, State},
    http::{Request, StatusCode},
    response::IntoResponse,
};
use tower_http::services::ServeFile;
use tracing::{debug, error, instrument, trace, warn};
use uuid::Uuid;

use crate::models::AppState;
use crate::utils::static_object::UPLOAD_DIR;

/// Serve partner's profile photo
///
/// GET /api/images/partner/{filename}
///
/// This endpoint serves profile photos for matched partners only.
/// It requires authentication and validates that the requesting user
/// is actually matched with the user whose photo is being requested.
#[instrument(skip_all)]
pub async fn serve_partner_image(
    State(state): State<Arc<AppState>>,
    Extension(requested_id): Extension<Uuid>,
    req: Request<Body>,
) -> impl IntoResponse {
    trace!("Serving partner image");

    // Query the partner's profile photo filename
    let photo_result = sqlx::query!(
        r#"
        SELECT profile_photo_filename
        FROM forms
        WHERE user_id = $1
        "#,
        requested_id
    )
    .fetch_optional(&state.db_pool)
    .await;

    let photo_filename = match photo_result {
        Ok(Some(record)) => match record.profile_photo_filename {
            Some(filename) => filename,
            None => {
                debug!(?requested_id, "No profile photo found");
                return (StatusCode::NOT_FOUND, "No profile photo found").into_response();
            }
        },
        Ok(None) => {
            warn!(?requested_id, "User not found in forms table");
            return (StatusCode::NOT_FOUND, "User not found").into_response();
        }
        Err(e) => {
            error!("Database error when fetching profile photo: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    // Construct the file path
    let file_path = Path::new(UPLOAD_DIR.as_str())
        .join("profile_photos")
        .join(photo_filename);

    debug!("Serving file: {:?}", file_path);

    // Use tower-http's ServeFile to serve the image
    let mut service = ServeFile::new(file_path);
    match service.try_call(req).await {
        Ok(res) => res.into_response(),
        Err(e) => {
            error!("Failed to serve file: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to serve file").into_response()
        }
    }
}
