//! # Upload Utilities
//!
//! This module provides common utilities for file uploads, specifically for image processing
//! and file management. These utilities are shared between different upload handlers to
//! ensure consistent validation and file handling.

use std::path::Path;

use image::{GenericImageView, ImageFormat, imageops::FilterType};
use tokio::{fs, io::AsyncWriteExt};
use tracing::{debug, trace};

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
    pub fn validate_content_type(content_type: &str) -> Result<(), &'static str> {
        if !content_type.starts_with("image/") {
            return Err("File must be an image (image/* content type required)");
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
    pub fn validate_image_format(data: &[u8]) -> Result<(&'static str, ImageFormat), &'static str> {
        let image_format =
            image::guess_format(data).map_err(|_| "Could not detect image format")?;

        let (file_extension, is_allowed_format) = match image_format {
            ImageFormat::Png => ("png", true),
            ImageFormat::Jpeg => ("jpg", true),
            ImageFormat::WebP => ("webp", true),
            _ => ("unknown", false),
        };

        if !is_allowed_format {
            return Err("Only PNG, JPG, and WEBP formats are allowed");
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
}

/// Provides image processing utilities for thumbnails and resizing.
pub struct ImageProcessor;

impl ImageProcessor {
    /// Creates a thumbnail from image data, maintaining aspect ratio.
    ///
    /// The larger dimension (width or height) is resized to the target size,
    /// and the other dimension is scaled proportionally. If the image is already
    /// smaller than or equal to the target size in both dimensions, it is returned as-is.
    ///
    /// # Arguments
    ///
    /// * `image_data` - The original image data
    /// * `target_size` - The target size for the larger dimension (e.g., 80)
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<u8>)` - The thumbnail image data in the original format
    /// * `Err(String)` - Error message if processing fails
    pub fn create_thumbnail(image_data: &[u8], target_size: u32) -> Result<Vec<u8>, String> {
        // Load the image
        let img = image::load_from_memory(image_data)
            .map_err(|e| format!("Failed to load image: {}", e))?;

        // Determine the original dimensions
        let (width, height) = img.dimensions();
        trace!(
            original_width = width,
            original_height = height,
            "Original image dimensions"
        );

        // If image is already smaller than target size, return as-is
        if width <= target_size && height <= target_size {
            debug!(
                "Image already smaller than target size ({}x{} <= {}), using original",
                width, height, target_size
            );
            return Ok(image_data.to_vec());
        }

        // Calculate new dimensions maintaining aspect ratio
        let (new_width, new_height) = if width > height {
            let ratio = height as f32 / width as f32;
            (target_size, (target_size as f32 * ratio).round() as u32)
        } else {
            let ratio = width as f32 / height as f32;
            ((target_size as f32 * ratio).round() as u32, target_size)
        };

        debug!(
            new_width = new_width,
            new_height = new_height,
            "Resizing to thumbnail dimensions"
        );

        // Resize the image using Lanczos3 filter for high quality
        let thumbnail = img.resize(new_width, new_height, FilterType::Lanczos3);

        // Detect format from original data
        let format = image::guess_format(image_data)
            .map_err(|e| format!("Failed to detect image format: {}", e))?;

        // Encode the thumbnail to bytes
        let mut buffer = Vec::new();
        thumbnail
            .write_to(&mut std::io::Cursor::new(&mut buffer), format)
            .map_err(|e| format!("Failed to encode thumbnail: {}", e))?;

        trace!(
            thumbnail_size = buffer.len(),
            "Thumbnail created successfully"
        );
        Ok(buffer)
    }

    /// Generates a thumbnail filename from the original filename.
    ///
    /// # Arguments
    ///
    /// * `original_filename` - The original filename (e.g., "uuid.jpg")
    ///
    /// # Returns
    ///
    /// A thumbnail filename in the format `{stem}_thumb.{extension}`
    pub fn thumbnail_filename(original_filename: &str) -> String {
        let path = Path::new(original_filename);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
        format!("{}_thumb.{}", stem, ext)
    }
}
