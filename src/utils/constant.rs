//! # Application Constants
//!
//! This module defines configuration constants used throughout the Hilo application.
//! These constants control various timeouts, limits, and security settings.

use std::time::Duration;

/// Rate limit duration for email verification requests
pub const EMAIL_RATE_LIMIT: Duration = Duration::from_secs(3 * 60);

/// Expiration time for verification codes
pub const VERIFICATION_CODE_EXPIRY: Duration = Duration::from_secs(5 * 60);

/// When caches exceed this size, expired entries are cleaned up
/// to prevent unlimited memory growth.
pub const CACHE_CAPACITY: usize = 100;

/// Interval for automatic cache cleanup
pub const CACHE_CLEANUP_INTERVAL: Duration = Duration::from_secs(3 * 60);

/// Expiration time for JWT access tokens
pub const ACCESS_TOKEN_EXPIRY: Duration = Duration::from_secs(15 * 60);

/// Expiration time for JWT refresh tokens
pub const REFRESH_TOKEN_EXPIRY: Duration = Duration::from_secs(7 * 24 * 60 * 60); // 7 days

/// Maximum number of tags a user can select (familiar + aspirational combined).
/// Should be set in env var. This is only a fallback value.
pub const TOTAL_TAGS_FALLBACK: usize = 10;

/// Maximum length in **bytes** for text fields like self_intro and recent_topics
pub const MAX_TEXT_FIELD_LENGTH: usize = 4 * 200; // assume 4 bytes per char

/// Maximum length for WeChat ID
pub const MAX_WECHAT_ID_LENGTH: usize = 100;
