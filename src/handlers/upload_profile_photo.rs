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

use std::{path::Path, sync::Arc};

use axum::{
    Json,
    extract::{Extension, Multipart, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::Serialize;
use tracing::{debug, error, info, instrument, trace, warn};

use crate::{
    error::{AppError, AppResult},
    middleware::AuthUser,
    models::{AppState, UserStatus},
    utils::{
        constant::THUMBNAIL_SIZE,
        file::{FileManager, ImageProcessor, ImageUploadValidator},
        static_object::UPLOAD_DIR,
    },
};

/// Response structure for successful profile photo upload.
#[derive(Serialize)]
struct UploadProfilePhotoResponse {
    pub filename: String,
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
/// - `200 OK` with `UploadProfilePhotoResponse` - File uploaded successfully
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
) -> AppResult<impl IntoResponse> {
    debug!("Processing profile photo upload request");

    // Check user status - only verified and form_completed users can upload
    let user_status = UserStatus::query(&state.db_pool, &user.user_id).await?;

    if !user_status.can_fill_form() {
        warn!(current_status = %user_status, "User status doesn't allow profile photo upload");
        return Err(AppError::Forbidden(
            "User status doesn't allow profile photo upload",
        ));
    }

    // Extract file from multipart form
    let field = multipart
        .next_field()
        .await
        .map_err(|e| {
            error!(error = %e, "Error reading multipart form");
            AppError::BadRequest("Invalid multipart data")
        })?
        .ok_or_else(|| {
            warn!("No file provided in multipart form");
            AppError::BadRequest("No file provided")
        })?;

    // Validate content type
    let content_type = field.content_type().unwrap_or("");
    ImageUploadValidator::validate_content_type(content_type).map_err(|e| {
        warn!(content_type = %content_type, error = %e, "Invalid content type");
        AppError::BadRequest(e)
    })?;

    // Read file data
    let file_data = field.bytes().await.map_err(|e| {
        error!(error = %e, "Error reading file data");
        AppError::BadRequest("Error reading file")
    })?;

    // Validate file is not empty
    ImageUploadValidator::validate_file_not_empty(&file_data).map_err(|e| {
        warn!(error = %e, "Empty file uploaded");
        AppError::BadRequest(e)
    })?;

    // Validate format using image crate
    let (file_extension, image_format) = ImageUploadValidator::validate_image_format(&file_data)
        .map_err(|e| {
            warn!(error = %e, "Invalid image format");
            AppError::BadRequest(e)
        })?;

    trace!(format = ?image_format, size = file_data.len(), "Image validation passed");

    // Prepare file storage
    let profile_photos_dir = Path::new(UPLOAD_DIR.as_str()).join("profile_photos");
    FileManager::ensure_directory_exists(&profile_photos_dir)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create upload directory");
            AppError::Internal
        })?;

    let filename = FileManager::generate_user_filename(user.user_id, file_extension);
    let full_path = profile_photos_dir.join(&filename);

    debug!(file_path = %full_path.display(), "Saving profile photo");

    // Write file to disk
    FileManager::save_file(&full_path, &file_data)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to save file");
            AppError::Internal
        })?;

    info!(
        file_size = file_data.len(),
        "Profile photo uploaded successfully"
    );

    // Generate and save thumbnail (non-critical - log errors but don't fail)
    match ImageProcessor::create_thumbnail(&file_data, THUMBNAIL_SIZE) {
        Ok(thumbnail_data) => {
            let thumbnail_filename = ImageProcessor::thumbnail_filename(&filename);
            let thumbnail_path = profile_photos_dir.join(&thumbnail_filename);

            debug!(thumbnail_path = %thumbnail_path.display(), "Saving thumbnail");

            if let Err(e) = FileManager::save_file(&thumbnail_path, &thumbnail_data).await {
                error!(error = %e, "Failed to save thumbnail, continuing without it");
            } else {
                info!(
                    thumbnail_size = thumbnail_data.len(),
                    "Thumbnail created successfully"
                );
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to create thumbnail, continuing without it");
        }
    }

    // Return success response with filename and user data
    Ok((
        StatusCode::OK,
        Json(UploadProfilePhotoResponse { filename }),
    ))
}
