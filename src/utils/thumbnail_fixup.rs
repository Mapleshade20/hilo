//! # Thumbnail Fixup Utility
//!
//! This module provides functionality to generate missing thumbnails for existing profile photos.
//! It's designed to run on server startup to ensure backward compatibility when the thumbnail
//! feature is first deployed.

use std::path::Path;

use tokio::fs;
use tracing::{debug, error, info, warn};

use crate::utils::{
    constant::THUMBNAIL_SIZE,
    file::{FileManager, ImageProcessor},
    static_object::UPLOAD_DIR,
};

/// Scans the profile_photos directory and generates thumbnails for any images that don't have them.
///
/// This function:
/// - Reads all files in the profile_photos directory
/// - Identifies images without corresponding _thumb files
/// - Generates and saves thumbnails for those images
/// - Logs progress and any errors
///
/// # Returns
///
/// Returns a tuple of (success_count, error_count) representing the number of thumbnails
/// successfully generated and the number of failures.
///
/// # Errors
///
/// Logs errors but continues processing remaining files. Does not propagate errors
/// to allow the server to start even if some thumbnails fail to generate.
pub async fn generate_missing_thumbnails() -> (usize, usize) {
    let profile_photos_dir = Path::new(UPLOAD_DIR.as_str()).join("profile_photos");

    // Check if directory exists
    if !profile_photos_dir.exists() {
        debug!(
            "Profile photos directory does not exist: {}",
            profile_photos_dir.display()
        );
        return (0, 0);
    }

    info!(
        "Starting thumbnail fixup scan in: {}",
        profile_photos_dir.display()
    );

    let mut success_count = 0;
    let mut error_count = 0;

    // Read directory entries
    let mut entries = match fs::read_dir(&profile_photos_dir).await {
        Ok(entries) => entries,
        Err(e) => {
            error!(
                "Failed to read profile photos directory: {}. Error: {}",
                profile_photos_dir.display(),
                e
            );
            return (0, 1);
        }
    };

    // Process each file
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        // Get filename
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                warn!("Skipping file with invalid UTF-8 filename: {:?}", path);
                continue;
            }
        };

        // Skip if this is already a thumbnail
        if filename.contains("_thumb.") {
            continue;
        }

        // Check if thumbnail already exists
        let thumbnail_filename = ImageProcessor::thumbnail_filename(filename);
        let thumbnail_path = profile_photos_dir.join(&thumbnail_filename);

        if thumbnail_path.exists() {
            debug!("Thumbnail already exists for: {}", filename);
            continue;
        }

        // Read the original image
        let image_data = match fs::read(&path).await {
            Ok(data) => data,
            Err(e) => {
                error!("Failed to read image file {}: {}", filename, e);
                error_count += 1;
                continue;
            }
        };

        // Generate thumbnail
        match ImageProcessor::create_thumbnail(&image_data, THUMBNAIL_SIZE) {
            Ok(thumbnail_data) => {
                // Save thumbnail
                match FileManager::save_file(&thumbnail_path, &thumbnail_data).await {
                    Ok(_) => {
                        info!("Generated thumbnail for: {}", filename);
                        success_count += 1;
                    }
                    Err(e) => {
                        error!("Failed to save thumbnail for {}: {}", filename, e);
                        error_count += 1;
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Failed to generate thumbnail for {} (may not be a valid image): {}",
                    filename, e
                );
                error_count += 1;
            }
        }
    }

    info!(
        "Thumbnail fixup complete. Generated: {}, Errors: {}",
        success_count, error_count
    );

    (success_count, error_count)
}
