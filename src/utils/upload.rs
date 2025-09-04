//! # Upload Utilities
//!
//! This module provides common utilities for file uploads, specifically for image processing
//! and file management. These utilities are shared between different upload handlers to
//! ensure consistent validation and file handling.

use image::ImageFormat;
use std::path::Path;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, trace};

/// Provides image validation utilities for upload handlers.
pub struct ImageUploadValidator;

impl ImageUploadValidator {
    /// Validates that the content type is an image type.
    ///
    /// # Arguments
    ///
    /// * `content_type` - The HTTP content type to validate
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Content type is valid (starts with "image/")
    /// * `Err(String)` - Content type is invalid with error message
    pub fn validate_content_type(content_type: &str) -> Result<(), String> {
        if !content_type.starts_with("image/") {
            return Err(format!(
                "File must be an image (image/* content type required), got: {content_type}"
            ));
        }
        Ok(())
    }

    /// Validates the image format and returns the file extension and format.
    ///
    /// # Arguments
    ///
    /// * `data` - The image file data to validate
    ///
    /// # Returns
    ///
    /// * `Ok((extension, format))` - Valid image with file extension and ImageFormat
    /// * `Err(String)` - Invalid image format with error message
    pub fn validate_image_format(data: &[u8]) -> Result<(&'static str, ImageFormat), String> {
        let image_format =
            image::guess_format(data).map_err(|e| format!("Could not detect image format: {e}"))?;

        let (file_extension, is_allowed_format) = match image_format {
            ImageFormat::Png => ("png", true),
            ImageFormat::Jpeg => ("jpg", true),
            ImageFormat::WebP => ("webp", true),
            _ => ("unknown", false),
        };

        if !is_allowed_format {
            return Err(format!(
                "Only PNG, JPG, and WEBP formats are allowed, got: {image_format:?}"
            ));
        }

        trace!(format = ?image_format, extension = file_extension, "Image format validated");
        Ok((file_extension, image_format))
    }

    /// Validates that the file is not empty.
    ///
    /// # Arguments
    ///
    /// * `data` - The file data to validate
    ///
    /// # Returns
    ///
    /// * `Ok(())` - File has content
    /// * `Err(String)` - File is empty with error message
    pub fn validate_file_not_empty(data: &[u8]) -> Result<(), &'static str> {
        if data.is_empty() {
            return Err("Empty file not allowed");
        }
        Ok(())
    }
}

/// Provides file system utilities for upload handlers.
pub struct FileManager;

impl FileManager {
    /// Ensures the specified directory exists, creating it if necessary.
    ///
    /// # Arguments
    ///
    /// * `path` - The directory path to ensure exists
    ///
    /// # Returns
    ///
    /// * `Ok(())` - Directory exists or was created successfully
    /// * `Err(std::io::Error)` - Failed to create directory
    pub async fn ensure_directory_exists(path: &Path) -> Result<(), std::io::Error> {
        trace!(path = %path.display(), "Ensuring directory exists");
        fs::create_dir_all(path).await
    }

    /// Generates a filename for a user's uploaded file.
    ///
    /// # Arguments
    ///
    /// * `user_id` - The user's UUID
    /// * `extension` - The file extension (without dot)
    ///
    /// # Returns
    ///
    /// A filename in the format `{uuid}.{extension}`
    pub fn generate_user_filename(user_id: uuid::Uuid, extension: &str) -> String {
        format!("{user_id}.{extension}")
    }

    /// Parses a UUID from the filename in the given path.
    ///
    /// Expects the filename to be in the format `{uuid}.{extension}`.
    pub fn parse_uuid_from_path(path: impl AsRef<Path>) -> Option<uuid::Uuid> {
        path.as_ref()
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|s| uuid::Uuid::try_parse(s).ok())
    }

    /// Saves file data to the specified path.
    ///
    /// # Arguments
    ///
    /// * `file_path` - The complete path where the file should be saved
    /// * `data` - The file data to save
    ///
    /// # Returns
    ///
    /// * `Ok(())` - File saved successfully
    /// * `Err(std::io::Error)` - Failed to save file
    pub async fn save_file(file_path: &Path, data: &[u8]) -> Result<(), std::io::Error> {
        debug!(file_path = %file_path.display(), size = data.len(), "Saving file");

        let mut file = fs::File::create(file_path).await?;
        file.write_all(data).await?;

        debug!(file_path = %file_path.display(), "File saved successfully");
        Ok(())
    }

    /// Attempts to clean up a file (used for error recovery).
    ///
    /// # Arguments
    ///
    /// * `file_path` - The path to the file to clean up
    ///
    /// This function logs errors but doesn't return them, as it's used for cleanup
    /// in error scenarios where the original error should be preserved.
    pub async fn cleanup_file(file_path: &Path) {
        if let Err(e) = fs::remove_file(file_path).await {
            error!(
                file_path = %file_path.display(),
                error = %e,
                "Failed to clean up file during error recovery"
            );
        } else {
            debug!(file_path = %file_path.display(), "File cleaned up successfully");
        }
    }
}
