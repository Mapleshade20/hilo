//! # Application Constants
//!
//! This module defines configuration constants used throughout the Hilo application.
//! These constants control various timeouts, limits, and security settings.

use std::time::Duration;

/// Rate limit duration for email verification requests
///
/// Users must wait this duration between verification code requests
/// to prevent abuse of the email service.
pub const EMAIL_RATE_LIMIT: Duration = Duration::from_secs(3 * 60);

/// Expiration time for verification codes
///
/// Verification codes become invalid after this duration for security.
pub const VERIFICATION_CODE_EXPIRY: Duration = Duration::from_secs(5 * 60);

/// Maximum number of entries to keep in memory caches
///
/// When caches exceed this size, expired entries are cleaned up
/// to prevent unlimited memory growth.
pub const CACHE_CAPACITY: usize = 100;

/// Interval for automatic cache cleanup
///
/// Background task runs at this interval to remove expired cache entries.
pub const CACHE_CLEANUP_INTERVAL: Duration = Duration::from_secs(3 * 60);

/// Expiration time for JWT access tokens
///
/// Access tokens are short-lived for security and must be refreshed regularly.
pub const ACCESS_TOKEN_EXPIRY: Duration = Duration::from_secs(15 * 60);

/// Expiration time for JWT refresh tokens
///
/// Refresh tokens have longer validity to reduce frequent re-authentication.
pub const REFRESH_TOKEN_EXPIRY: Duration = Duration::from_secs(7 * 24 * 60 * 60); // 7 days
