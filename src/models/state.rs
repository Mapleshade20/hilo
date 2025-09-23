use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use sqlx::PgPool;
use tracing::{debug, info, instrument};

use crate::services::{email::EmailService, jwt::JwtService};
use crate::utils::constant::*;

/// Application state shared across requests. Needs to be thread-safe.
pub struct AppState {
    /// A map of email addresses to their rate limit timestamps.
    pub rate_limit_cache: DashMap<String, Instant>,
    /// A map of email addresses to their verification codes and timestamps.
    pub verification_code_cache: DashMap<String, (String, Instant)>,
    /// The email service used to send verification codes.
    pub email_service: Arc<dyn EmailService>,
    /// The PostgreSQL database connection pool.
    pub db_pool: PgPool,
    /// JWT service for token generation and validation.
    pub jwt_service: JwtService,
}

impl AppState {
    /// Creates a new application state with the provided services.
    ///
    /// # Arguments
    ///
    /// * `email_service` - Service for sending verification emails
    /// * `db_pool` - PostgreSQL database connection pool
    /// * `jwt_service` - Service for JWT token operations
    pub fn new(
        email_service: Arc<dyn EmailService>,
        db_pool: PgPool,
        jwt_service: JwtService,
    ) -> Self {
        info!("Initializing application state");
        debug!(
            cache_capacity = CACHE_CAPACITY,
            "Creating new caches with configured capacity"
        );

        Self {
            rate_limit_cache: DashMap::new(),
            verification_code_cache: DashMap::new(),
            email_service,
            db_pool,
            jwt_service,
        }
    }

    /// Cleans up expired entries from both verification code and rate limit caches.
    ///
    /// This method is called periodically to prevent memory leaks from expired entries.
    /// Only performs cleanup when cache size exceeds the configured capacity.
    #[instrument(skip_all)]
    pub fn cleanup_expired_entries(&self) {
        let verification_cache_size = self.verification_code_cache.len();
        let rate_limit_cache_size = self.rate_limit_cache.len();

        debug!(
            verification_cache_size,
            rate_limit_cache_size,
            cache_capacity = CACHE_CAPACITY,
            "Checking if cache cleanup is needed"
        );

        if verification_cache_size > CACHE_CAPACITY {
            let initial_size = verification_cache_size;
            self.verification_code_cache
                .retain(|_, (_, timestamp)| timestamp.elapsed() <= VERIFICATION_CODE_EXPIRY);
            let final_size = self.verification_code_cache.len();

            info!(
                initial_size,
                final_size,
                removed = initial_size - final_size,
                "Cleaned up expired verification code entries"
            );
        }

        if rate_limit_cache_size > CACHE_CAPACITY {
            let initial_size = rate_limit_cache_size;
            self.rate_limit_cache
                .retain(|_, timestamp| timestamp.elapsed() <= EMAIL_RATE_LIMIT);
            let final_size = self.rate_limit_cache.len();

            info!(
                initial_size,
                final_size,
                removed = initial_size - final_size,
                "Cleaned up expired rate limit entries"
            );
        }
    }
}
