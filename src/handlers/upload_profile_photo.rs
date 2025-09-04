//! # Profile Photo Upload Handler
//!
//! This module implements the HTTP handler for profile photo uploads. Unlike card uploads,
//! profile photos are only available to verified and form_completed users, and the filename
//! is returned in the response rather than stored in the database.
//!
//! # Access Control
//!
//! Only users with 'verified' or 'form_completed' status can upload profile photos.
//!
//! # File Storage
//!
//! Files are stored in `UPLOAD_DIR/profile_photos/{user_uuid}.{ext}`
//! The file path is returned in the JSON response but NOT stored in the database.

use std::path::Path;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Multipart, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Serialize;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::middleware::AuthUser;
use crate::models::{AppState, UserStatus};
use crate::utils::static_object::UPLOAD_DIR;
use crate::utils::upload::{FileManager, ImageUploadValidator};

/// Response structure for successful profile photo upload.
#[derive(Serialize)]
struct UploadProfilePhotoResponse {
    filename: String,
}

/// Uploads a profile photo for verified users.
///
/// POST /api/upload/profile-photo MultipartForm
///
/// This endpoint accepts multipart/form-data with an image file and stores it
/// for the user's profile. Only users with 'verified' or 'form_completed' status
/// can upload profile photos.
///
/// # Security & Validation
///
/// - Content-Type must be image/*
/// - Image format must be PNG, JPG, or WEBP (validated with image crate)
/// - Files are stored with UUID-based names to prevent conflicts
/// - File path is returned in response but NOT stored in database
///
/// # File Storage
///
/// Files are stored in `UPLOAD_DIR/profile_photos/{user_uuid}.{ext}`
/// where the directory is created if it doesn't exist.
///
/// # Returns
///
/// - `200 OK` - File uploaded successfully with JSON response containing file path
/// - `400 Bad Request` - Invalid file format or missing file
/// - `403 Forbidden` - User status doesn't allow upload
/// - `413 Payload Too Large` - File exceeds 2MB limit (handled by Axum)
/// - `500 Internal Server Error` - File system error
#[instrument(
    skip_all,
    fields(
        user_id = %user.user_id,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn upload_profile_photo(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    debug!("Processing profile photo upload request");

    // 1. Check user status - only verified and form_completed users can upload
    let user_status = match UserStatus::query(&state.db_pool, &user.user_id).await {
        Ok(status) => status,
        Err(resp) => {
            error!("Failed to query user status from database");
            return resp.into_response();
        }
    };

    if !user_status.can_fill_form() {
        warn!(current_status = %user_status, "User status doesn't allow profile photo upload");
        return (
            StatusCode::FORBIDDEN,
            "User status doesn't allow profile photo upload",
        )
            .into_response();
    }

    // 2. Extract file from multipart form
    let field = match multipart.next_field().await {
        Ok(Some(field)) => field,
        Ok(None) => {
            warn!("No file provided in multipart form");
            return (StatusCode::BAD_REQUEST, "No file provided").into_response();
        }
        Err(e) => {
            error!(error = %e, "Error reading multipart form");
            return (StatusCode::BAD_REQUEST, "Invalid multipart data").into_response();
        }
    };

    // 3. Validate content type
    let content_type = field.content_type().unwrap_or("");
    if let Err(e) = ImageUploadValidator::validate_content_type(content_type) {
        warn!(content_type = %content_type, error = %e, "Invalid content type");
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    // 4. Read file data
    let file_data = match field.bytes().await {
        Ok(data) => data,
        Err(e) => {
            error!(error = %e, "Error reading file data");
            return (StatusCode::BAD_REQUEST, "Error reading file").into_response();
        }
    };

    // 5. Validate file is not empty
    if let Err(e) = ImageUploadValidator::validate_file_not_empty(&file_data) {
        warn!(error = %e, "Empty file uploaded");
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    // 6. Validate format using image crate
    let (file_extension, image_format) =
        match ImageUploadValidator::validate_image_format(&file_data) {
            Ok((ext, format)) => (ext, format),
            Err(e) => {
                warn!(error = %e, "Invalid image format");
                return (StatusCode::BAD_REQUEST, e).into_response();
            }
        };

    trace!(format = ?image_format, size = file_data.len(), "Image validation passed");

    // 7. Prepare file storage
    let profile_photos_dir = Path::new(UPLOAD_DIR.as_str()).join("profile_photos");
    if let Err(e) = FileManager::ensure_directory_exists(&profile_photos_dir).await {
        error!(error = %e, "Failed to create upload directory");
        return (StatusCode::INTERNAL_SERVER_ERROR, "File system error").into_response();
    }

    let filename = FileManager::generate_user_filename(user.user_id, file_extension);
    let full_path = profile_photos_dir.join(&filename);

    debug!(file_path = %full_path.display(), "Saving profile photo");

    // 8. Write file to disk
    if let Err(e) = FileManager::save_file(&full_path, &file_data).await {
        error!(error = %e, "Failed to save file");
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save file").into_response();
    }

    info!(
        file_size = file_data.len(),
        file_path = %full_path.display(),
        "Profile photo uploaded successfully"
    );

    // Return success response with filename (without database update, which should be handled
    // when user submits their form)
    (
        StatusCode::OK,
        Json(UploadProfilePhotoResponse { filename }),
    )
        .into_response()
}
