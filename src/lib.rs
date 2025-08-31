//! # Hilo - Social Pairing Backend
//!
//! ## Modules
//!
//! - [`handlers`] - HTTP request handlers for various endpoints
//! - [`middleware`] - Custom middleware for authentication and other cross-cutting concerns
//! - [`services`] - Business logic services (email, JWT, etc.)
//! - [`utils`] - Utility functions and constants

pub mod handlers;
pub mod middleware;
pub mod models;
pub mod services;
pub mod utils;

use std::env;
use std::sync::Arc;

use axum::{
    Router,
    middleware::from_fn_with_state,
    routing::{get, post},
};
use jsonwebtoken::{DecodingKey, EncodingKey};
use secrecy::{ExposeSecret, SecretSlice};
use sqlx::PgPool;
use tracing::info;

use crate::handlers::{
    get_form, get_profile, health_check, refresh_token, send_verification_code, submit_form,
    upload_card, upload_profile_photo, verify_code,
};
use crate::middleware::auth_middleware;
use crate::models::{AppState, TagSystem};
use crate::services::email::{EmailService, ExternalEmailer, LogEmailer};
use crate::services::jwt::JwtService;
use crate::utils::constant::*;

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
            .expect("Env variable `APP_ENV` should be set")
            .to_ascii_lowercase();

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

    let tag_system =
        TagSystem::from_json("tags.json").expect("Failed to load tag system from tags.json");

    let state = Arc::new(AppState::new(
        email_service,
        db_pool,
        jwt_service,
        tag_system,
    ));

    let state_clone = Arc::clone(&state);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CACHE_CLEANUP_INTERVAL);
        interval.tick().await; // first tick completes immediately
        loop {
            interval.tick().await;
            state_clone.cleanup_expired_entries();
        }
    });

    let protected_routes = Router::new()
        .route("/api/profile", get(get_profile))
        .route("/api/form", post(submit_form))
        .route("/api/form", get(get_form))
        .route("/api/upload/card", post(upload_card))
        .route("/api/upload/profile-photo", post(upload_profile_photo))
        .route_layer(from_fn_with_state(Arc::clone(&state), auth_middleware));

    let public_routes = Router::new()
        .route("/health-check", get(health_check))
        .route("/api/auth/send-code", post(send_verification_code))
        .route("/api/auth/verify-code", post(verify_code))
        .route("/api/auth/refresh", post(refresh_token));

    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .with_state(state)
}
