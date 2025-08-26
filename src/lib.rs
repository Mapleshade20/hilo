//! # Hilo - Social Matching Backend
//!
//! ## Modules
//!
//! - [`handlers`] - HTTP request handlers for various endpoints
//! - [`middleware`] - Custom middleware for authentication and other cross-cutting concerns
//! - [`services`] - Business logic services (email, JWT, etc.)
//! - [`utils`] - Utility functions and constants

pub mod handlers;
pub mod middleware;
pub mod services;
pub mod utils;

use std::borrow::Cow;
use std::env;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    Router,
    routing::{get, post},
};
use dashmap::DashMap;
use jsonwebtoken::{DecodingKey, EncodingKey};
use secrecy::{ExposeSecret, SecretSlice};
use sqlx::PgPool;
use tracing::info;

use crate::handlers::{health_check, refresh_token, send_verification_code, verify_code};
use crate::services::email::{EmailService, ExternalEmailer, LogEmailer};
use crate::services::jwt::JwtService;
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
    pub fn cleanup_expired_entries(&self) {
        if self.verification_code_cache.len() > CACHE_CAPACITY {
            self.verification_code_cache
                .retain(|_, (_, timestamp)| timestamp.elapsed() <= VERIFICATION_CODE_EXPIRY);
        }
        if self.rate_limit_cache.len() > CACHE_CAPACITY {
            self.rate_limit_cache
                .retain(|_, timestamp| timestamp.elapsed() <= EMAIL_RATE_LIMIT);
        }
    }
}

/// Creates an Axum router with default email service configuration.
///
/// This is a convenience function that calls [`app_with_email_service`] with no custom email service,
/// causing it to auto-detect the appropriate email service based on the `APP_ENV` environment variable.
#[inline]
pub fn app(db_pool: PgPool) -> Router {
    app_with_email_service(db_pool, None)
}

/// Creates an Axum router with application routes and state.
///
/// # Arguments
///
/// * `db_pool` - PostgreSQL database connection pool
/// * `email_service` - Optional custom email service. If None, will auto-detect based on APP_ENV
///
/// # Environment Variables
///
/// - `APP_ENV` - "production" uses ExternalEmailer, otherwise uses LogEmailer (mock)
/// - `MAIL_API_URL` - Required in production for external email service
/// - `MAIL_API_KEY` - Required in production for external email service
/// - `SENDER_EMAIL` - Required in production for external email service
/// - `JWT_SECRET` - Required for JWT token signing and validation
///
/// # Returns
///
/// A configured Axum router with all application routes and middleware
pub fn app_with_email_service(
    db_pool: PgPool,
    email_service: Option<Arc<dyn EmailService>>,
) -> Router {
    let email_service: Arc<dyn EmailService> = if let Some(service) = email_service {
        service
    } else {
        let app_env = env::var("APP_ENV")
            .map(Cow::Owned)
            .unwrap_or_else(|_| Cow::Borrowed("development"));

        if app_env == "production" {
            info!("Running in production mode with [ExternalEmailer]");
            let api_url =
                env::var("MAIL_API_URL").expect("Env variable `MAIL_API_URL` should be set");
            let api_key =
                env::var("MAIL_API_KEY").expect("Env variable `MAIL_API_KEY` should be set");
            let sender =
                env::var("SENDER_EMAIL").expect("Env variable `SENDER_EMAIL` should be set");
            Arc::new(ExternalEmailer::new(api_url, api_key, sender))
        } else {
            info!("Running in development mode with [LogEmailer (Mock)]");
            Arc::new(LogEmailer)
        }
    };

    let jwt_keys = SecretSlice::from(
        env::var("JWT_SECRET")
            .expect("Env variable `JWT_SECRET` should be set")
            .into_bytes(),
    );

    let jwt_service = JwtService::new(
        EncodingKey::from_secret(jwt_keys.expose_secret()),
        DecodingKey::from_secret(jwt_keys.expose_secret()),
    );

    let state = Arc::new(AppState::new(email_service, db_pool, jwt_service));
    let state_clone = Arc::clone(&state);

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CACHE_CLEANUP_INTERVAL);
        interval.tick().await; // first tick completes immediately
        loop {
            interval.tick().await;
            state_clone.cleanup_expired_entries();
        }
    });

    Router::new()
        .route("/health-check", get(health_check))
        .route("/api/auth/send-code", post(send_verification_code))
        .route("/api/auth/verify-code", post(verify_code))
        .route("/api/auth/refresh", post(refresh_token))
        .with_state(state)
}
