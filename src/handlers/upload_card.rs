//! # Upload Handlers
//!
//! This module implements HTTP handlers for file uploads, specifically for student card verification.
//! The upload flow is part of the user verification process:
//!
//! 1. Only users with 'unverified' status can upload cards
//! 2. Images are validated for format (PNG/JPG/WEBP) and content type
//! 3. Files are stored in the filesystem with secure naming
//! 4. Database is updated with the file path for admin review

use std::path::Path;
use std::sync::Arc;

use axum::{
    extract::{Extension, Multipart, State},
    http::StatusCode,
    response::IntoResponse,
};
use tracing::{debug, error, info, instrument, trace, warn};

use crate::middleware::AuthUser;
use crate::models::{AppState, UserStatus};
use crate::utils::static_object::{ALLOWED_GRADES, UPLOAD_DIR};
use crate::utils::upload::{FileManager, ImageUploadValidator};

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
/// - `200 OK` - File uploaded successfully
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
) -> impl IntoResponse {
    debug!("Processing card upload request");

    // Check user status - only unverified users can upload
    let user_status = match UserStatus::query(&state.db_pool, &user.user_id).await {
        Ok(status) => status,
        Err(resp) => {
            error!("Failed to query user status from database");
            return resp.into_response();
        }
    };

    if !user_status.can_upload_card() {
        warn!(current_status = %user_status, "User status doesn't allow card upload");
        return (
            StatusCode::FORBIDDEN,
            "User status doesn't allow card upload",
        )
            .into_response();
    }

    // Extract fields from multipart form
    let mut file_data = None;
    let mut grade = None;

    while let Some(field) = match multipart.next_field().await {
        Ok(field) => field,
        Err(e) => {
            error!(error = %e, "Error reading multipart form");
            return (StatusCode::BAD_REQUEST, "Invalid multipart data").into_response();
        }
    } {
        let field_name = field.name().unwrap_or("");

        match field_name {
            "card" => {
                // Validate content type
                let content_type = field.content_type().unwrap_or("");
                if let Err(e) = ImageUploadValidator::validate_content_type(content_type) {
                    warn!(content_type = %content_type, error = %e, "Invalid content type");
                    return (StatusCode::BAD_REQUEST, e).into_response();
                }

                // Read file data
                file_data = Some(match field.bytes().await {
                    Ok(data) => data,
                    Err(e) => {
                        error!(error = %e, "Error reading file data");
                        return (StatusCode::BAD_REQUEST, "Error reading file").into_response();
                    }
                });
            }
            "grade" => {
                let grade_local = match field.text().await {
                    Ok(text) => text,
                    Err(e) => {
                        error!(error = %e, "Error reading grade field");
                        return (StatusCode::BAD_REQUEST, "Error reading grade").into_response();
                    }
                };

                // Validate grade
                if !ALLOWED_GRADES.contains(&grade_local.as_str()) {
                    warn!(grade = %grade_local, "Invalid grade");
                    return (StatusCode::BAD_REQUEST, "Error reading grade").into_response();
                }

                grade = Some(grade_local);
            }
            _ => {
                warn!(field_name = %field_name, "Unknown field in multipart form");
            }
        }
    }

    // Validate required fields
    let file_data = match file_data {
        Some(data) => data,
        None => {
            warn!("No file provided in multipart form");
            return (StatusCode::BAD_REQUEST, "No file provided").into_response();
        }
    };

    let grade = match grade {
        Some(g) => g,
        None => {
            warn!("No grade provided in multipart form");
            return (StatusCode::BAD_REQUEST, "Grade field is required").into_response();
        }
    };

    // Validate file is not empty
    if let Err(e) = ImageUploadValidator::validate_file_not_empty(&file_data) {
        warn!(error = %e, "Empty file uploaded");
        return (StatusCode::BAD_REQUEST, e).into_response();
    }

    // Validate image format using image crate
    let (file_extension, image_format) =
        match ImageUploadValidator::validate_image_format(&file_data) {
            Ok((ext, format)) => (ext, format),
            Err(e) => {
                warn!(error = %e, "Invalid image format");
                return (StatusCode::BAD_REQUEST, e).into_response();
            }
        };

    trace!(format = ?image_format, size = file_data.len(), "Image validation passed");

    // Prepare file storage
    let card_photos_dir = Path::new(UPLOAD_DIR.as_str()).join("card_photos");
    if let Err(e) = FileManager::ensure_directory_exists(&card_photos_dir).await {
        error!(error = %e, "Failed to create upload directory");
        return (StatusCode::INTERNAL_SERVER_ERROR, "File system error").into_response();
    }
    let filename = FileManager::generate_user_filename(user.user_id, file_extension);
    let full_path = card_photos_dir.join(&filename);

    debug!(file_path = %full_path.display(), grade = %grade, "Saving file");

    // Write file to disk
    if let Err(e) = FileManager::save_file(&full_path, &file_data).await {
        error!(error = %e, "Failed to save file");
        return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save file").into_response();
    }

    // Update database with file path, grade, and status
    match sqlx::query!(
        "UPDATE users SET card_photo_filename = $1, grade = $2, status = $3 WHERE id = $4",
        filename,
        grade,
        UserStatus::VerificationPending as UserStatus,
        user.user_id
    )
    .execute(&state.db_pool)
    .await
    {
        Ok(_) => {
            info!(
                file_size = file_data.len(),
                grade = %grade,
                "Card uploaded successfully, status updated to verification_pending"
            );
        }
        Err(e) => {
            error!(error = %e, "Failed to update database with file path and status");
            // Try to clean up the file
            FileManager::cleanup_file(&full_path).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    }

    (StatusCode::OK, "Verification pending").into_response()
}
