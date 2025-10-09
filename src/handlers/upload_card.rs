//! # Upload Handlers
//!
//! This module implements HTTP handlers for file uploads, specifically for student card verification.
//! The upload flow is part of the user verification process:
//!
//! 1. Only users with 'unverified' status can upload cards
//! 2. Images are validated for format (PNG/JPG/WEBP) and content type
//! 3. Files are stored in the filesystem with secure naming
//! 4. Database is updated with the file path for admin review

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
        file::{FileManager, ImageUploadValidator},
        static_object::{ALLOWED_GRADES, UPLOAD_DIR},
    },
};

/// Response structure for successful card upload
#[derive(Serialize)]
pub struct CardUploadResponse {
    pub email: String,
    pub status: UserStatus,
    pub grade: String,
    pub card_photo_filename: String,
}

/// Uploads a student card for verification.
///
/// POST /api/upload/card MultipartForm
///
/// This endpoint accepts multipart/form-data with a `card` image field and a
/// `grade` text field, and stores it for admin verification. Only users with
/// 'unverified' status can upload cards.
///
/// # Security & Validation
///
/// - Content-Type must be image/*
/// - Image format must be PNG, JPG, or WEBP (validated with image crate)
/// - Files are stored with UUID-based names to prevent conflicts
/// - File path is stored in database for admin review
///
/// # File Storage
///
/// Files are stored in `UPLOAD_DIR/card_photos/{user_uuid}.{ext}`
/// where the directory is created if it doesn't exist.
///
/// # Returns
///
/// - `200 OK` with `CardUploadResponse` - File uploaded successfully
/// - `400 Bad Request` - Invalid file format or missing file
/// - `403 Forbidden` - User status doesn't allow upload
/// - `413 Payload Too Large` - File exceeds 2MB limit (handled by Axum)
/// - `500 Internal Server Error` - File system or database error
#[instrument(
    skip_all,
    fields(
        user_id = %user.user_id,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn upload_card(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    mut multipart: Multipart,
) -> AppResult<impl IntoResponse> {
    debug!("Processing card upload request");

    // Check user status - only unverified users can upload
    let user_status = UserStatus::query(&state.db_pool, &user.user_id).await?;

    if !user_status.can_upload_card() {
        warn!(current_status = %user_status, "User status doesn't allow card upload");
        return Err(AppError::Forbidden("User status doesn't allow card upload"));
    }

    // Extract fields from multipart form
    let mut file_data = None;
    let mut grade = None;

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        error!(error = %e, "Error reading multipart form");
        AppError::BadRequest("Invalid multipart data")
    })? {
        let field_name = field.name().unwrap_or("");

        match field_name {
            "card" => {
                // Validate content type
                let content_type = field.content_type().unwrap_or("");
                ImageUploadValidator::validate_content_type(content_type).map_err(|e| {
                    warn!(content_type = %content_type, error = %e, "Invalid content type");
                    AppError::BadRequest(e)
                })?;

                // Read file data
                file_data = Some(field.bytes().await.map_err(|e| {
                    error!(error = %e, "Error reading file data");
                    AppError::BadRequest("Error reading file")
                })?);
            }
            "grade" => {
                let grade_local = field.text().await.map_err(|e| {
                    error!(error = %e, "Error reading grade field");
                    AppError::BadRequest("Error reading grade")
                })?;

                // Validate grade
                if !ALLOWED_GRADES.contains(&grade_local.as_str()) {
                    warn!(grade = %grade_local, "Invalid grade");
                    return Err(AppError::BadRequest("Invalid grade"));
                }

                grade = Some(grade_local);
            }
            _ => {
                warn!(field_name = %field_name, "Unknown field in multipart form");
            }
        }
    }

    // Validate required fields
    let file_data = file_data.ok_or_else(|| {
        warn!("No file provided in multipart form");
        AppError::BadRequest("No file provided")
    })?;

    let grade = grade.ok_or_else(|| {
        warn!("No grade provided in multipart form");
        AppError::BadRequest("Grade field is required")
    })?;

    // Validate file is not empty
    ImageUploadValidator::validate_file_not_empty(&file_data).map_err(|e| {
        warn!(error = %e, "Empty file uploaded");
        AppError::BadRequest(e)
    })?;

    // Validate image format using image crate
    let (file_extension, image_format) = ImageUploadValidator::validate_image_format(&file_data)
        .map_err(|e| {
            warn!(error = %e, "Invalid image format");
            AppError::BadRequest(e)
        })?;

    trace!(format = ?image_format, size = file_data.len(), "Image validation passed");

    // Prepare file storage
    let card_photos_dir = Path::new(UPLOAD_DIR.as_str()).join("card_photos");
    FileManager::ensure_directory_exists(&card_photos_dir)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create upload directory");
            AppError::Internal
        })?;

    let filename = FileManager::generate_user_filename(user.user_id, file_extension);
    let full_path = card_photos_dir.join(&filename);

    debug!(file_path = %full_path.display(), grade = %grade, "Saving file");

    // Write file to disk
    FileManager::save_file(&full_path, &file_data)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to save file");
            AppError::Internal
        })?;

    // Update database with file path, grade, and status, returning user data
    let user_data = sqlx::query!(
        r#"UPDATE users SET card_photo_filename = $1, grade = $2, status = $3 WHERE id = $4
           RETURNING email, status as "status: UserStatus", grade, card_photo_filename"#,
        filename,
        grade,
        UserStatus::VerificationPending as UserStatus,
        user.user_id
    )
    .fetch_one(&state.db_pool)
    .await?;

    info!(
        file_size = file_data.len(),
        grade = %grade,
        "Card uploaded successfully, status updated to verification_pending"
    );

    Ok((
        StatusCode::OK,
        Json(CardUploadResponse {
            email: user_data.email,
            status: user_data.status,
            grade: user_data.grade.unwrap_or_default(),
            card_photo_filename: user_data.card_photo_filename.unwrap_or_default(),
        }),
    ))
}
