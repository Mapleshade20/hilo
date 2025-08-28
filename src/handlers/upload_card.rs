//! # Upload Handlers
//!
//! This module implements HTTP handlers for file uploads, specifically for student card verification.
//! The upload flow is part of the user verification process:
//!
//! 1. Only users with 'unverified' status can upload cards
//! 2. Images are validated for format (PNG/JPG/WEBP) and content type
//! 3. Files are stored in the filesystem with secure naming
//! 4. Database is updated with the file path for admin review

use std::env;
use std::path::Path;
use std::sync::Arc;

use axum::{
    extract::{Extension, Multipart, State},
    http::StatusCode,
    response::IntoResponse,
};
use image::ImageFormat;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, instrument, warn};

use crate::middleware::AuthUser;
use crate::state::AppState;
use crate::utils::user_status::UserStatus;

/// Uploads a student card image for verification.
///
/// This endpoint accepts multipart/form-data with an image file and stores it
/// for admin verification. Only users with 'unverified' status can upload cards.
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
/// Files are stored in `UPLOAD_DIR/card_photos/user_{user_uuid}.{ext}`
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
    skip(state, user, multipart),
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

    // 1. Check user status - only unverified users can upload
    let user_status = match sqlx::query_scalar!(
        "SELECT status as \"status: UserStatus\" FROM users WHERE id = $1",
        user.user_id
    )
    .fetch_optional(&state.db_pool)
    .await
    {
        Ok(Some(status)) => status,
        Ok(None) => {
            warn!("User not found in database");
            return (StatusCode::NOT_FOUND, "User not found").into_response();
        }
        Err(e) => {
            error!(error = %e, "Database error when checking user status");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    };

    if !user_status.can_upload_card() {
        warn!(current_status = %user_status, "User status doesn't allow card upload");
        return (
            StatusCode::FORBIDDEN,
            format!("Card upload not allowed for status: {user_status}"),
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
    if !content_type.starts_with("image/") {
        warn!(content_type = %content_type, "Invalid content type");
        return (
            StatusCode::BAD_REQUEST,
            "File must be an image (image/* content type required)",
        )
            .into_response();
    }

    // 4. Read file data
    let file_data = match field.bytes().await {
        Ok(data) => data,
        Err(e) => {
            error!(error = %e, "Error reading file data");
            return (StatusCode::BAD_REQUEST, "Error reading file").into_response();
        }
    };

    if file_data.is_empty() {
        warn!("Empty file uploaded");
        return (StatusCode::BAD_REQUEST, "Empty file not allowed").into_response();
    }

    // 5. Validate image format using image crate
    let image_format = match image::guess_format(&file_data) {
        Ok(format) => format,
        Err(e) => {
            warn!(error = %e, "Could not detect image format");
            return (
                StatusCode::BAD_REQUEST,
                "Invalid image format. Please upload PNG, JPG, or WEBP",
            )
                .into_response();
        }
    };

    let (file_extension, is_allowed_format) = match image_format {
        ImageFormat::Png => ("png", true),
        ImageFormat::Jpeg => ("jpg", true),
        ImageFormat::WebP => ("webp", true),
        _ => ("unknown", false),
    };

    if !is_allowed_format {
        warn!(format = ?image_format, "Unsupported image format");
        return (
            StatusCode::BAD_REQUEST,
            "Only PNG, JPG, and WEBP formats are allowed",
        )
            .into_response();
    }

    debug!(format = ?image_format, size = file_data.len(), "Image validation passed");

    // 6. Prepare file storage
    let upload_dir = env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".to_string());
    let card_photos_dir = Path::new(&upload_dir).join("card_photos");

    // Create directory if it doesn't exist
    if let Err(e) = fs::create_dir_all(&card_photos_dir).await {
        error!(error = %e, "Failed to create upload directory");
        return (StatusCode::INTERNAL_SERVER_ERROR, "File system error").into_response();
    }

    // 7. Generate file path
    let filename = format!("user_{}.{}", user.user_id, file_extension);
    let file_path = card_photos_dir.join(&filename);

    debug!(file_path = %file_path.display(), "Saving file");

    // 8. Write file to disk
    match fs::File::create(&file_path).await {
        Ok(mut file) => {
            if let Err(e) = file.write_all(&file_data).await {
                error!(error = %e, "Failed to write file");
                return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to save file").into_response();
            }
        }
        Err(e) => {
            error!(error = %e, "Failed to create file");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create file").into_response();
        }
    }

    // 9. Update database with file path and status
    match sqlx::query!(
        "UPDATE users SET card_photo_path = $1, status = $2 WHERE id = $3",
        file_path.to_str(),
        UserStatus::VerificationPending as UserStatus,
        user.user_id
    )
    .execute(&state.db_pool)
    .await
    {
        Ok(_) => {
            info!(
                file_name = %filename,
                file_size = file_data.len(),
                "Card uploaded successfully, status updated to verification_pending"
            );
        }
        Err(e) => {
            error!(error = %e, "Failed to update database with file path and status");
            // Try to clean up the file
            let _ = fs::remove_file(&file_path).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
        }
    }

    (StatusCode::OK, "Verification pending").into_response()
}
