//! # Hilo - Social Pairing Backend
//!
//! ## Modules
//!
//! - [`handlers`] - HTTP request handlers for various endpoints
//! - [`middleware`] - Custom middleware for authentication and other cross-cutting concerns
//! - [`services`] - Business logic services (email, JWT, etc.)
//! - [`utils`] - Utility functions and constants

pub mod error;
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
    routing::{delete, get, post},
};
use jsonwebtoken::{DecodingKey, EncodingKey};
use sqlx::PgPool;
use tracing::info;

use crate::handlers::{
    accept_final_match, add_veto, get_form, get_next_match_time, get_previews, get_profile,
    get_vetoes, health_check, refresh_token, reject_final_match, remove_veto,
    send_verification_code, serve_partner_image, submit_form, upload_card, upload_profile_photo,
    verify_code,
};
use crate::models::AppState;
use crate::services::email::{EmailService, ExternalEmailer, LogEmailer};
use crate::services::jwt::JwtService;
use crate::services::matching::MatchingService;
use crate::services::scheduler::SchedulerService;
use crate::utils::{constant::*, secret, static_object::TAG_SYSTEM};

/// Creates an Axum router with default email service configuration.
///
/// # Environment Variables
///
/// - `EMAIL_PROVIDER` - "external" uses ExternalEmailer, "log" uses LogEmailer (default)
/// - `MAIL_API_URL`   - Required in production for external email service
/// - `MAIL_API_KEY` or `MAIL_API_KEY_FILE` (preferred)  - Required for external email service
/// - `SENDER_EMAIL`   - Required in production for external email service
pub fn app(db_pool: PgPool) -> Router {
    let email_service: Arc<dyn EmailService> = match env::var("EMAIL_PROVIDER")
        .expect("Env variable `EMAIL_PROVIDER` should be set")
        .as_str()
    {
        "external" => {
            info!("Email provider set to [ExternalEmailer]");
            let api_url =
                env::var("MAIL_API_URL").expect("Env variable `MAIL_API_URL` should be set");
            let api_key = secret::get_secret("MAIL_API_KEY_FILE", "MAIL_API_KEY")
                .expect("Either `MAIL_API_KEY_FILE` or `MAIL_API_KEY` env variable should be set");
            let sender =
                env::var("SENDER_EMAIL").expect("Env variable `SENDER_EMAIL` should be set");
            Arc::new(ExternalEmailer::new(api_url, api_key, sender))
        }
        _ => {
            info!("Email provider set to [LogEmailer]");
            Arc::new(LogEmailer)
        }
    };

    app_with_email_service(db_pool, email_service)
}

/// Creates an Axum router with application routes and state.
///
/// # Arguments
///
/// * `db_pool` - PostgreSQL database connection pool
/// * `email_service` - Custom email service
///
/// # Environment Variables
///
/// - `JWT_SECRET` or `JWT_SECRET_FILE` (preferred) - For token signing and validation
///
/// # Returns
///
/// A configured Axum router with all application routes and middleware
pub fn app_with_email_service(db_pool: PgPool, email_service: Arc<dyn EmailService>) -> Router {
    let jwt_service = {
        let jwt_secret = secret::get_secret("JWT_SECRET_FILE", "JWT_SECRET")
            .expect("Either `JWT_SECRET_FILE` or `JWT_SECRET` env variable should be set");
        JwtService::new(
            EncodingKey::from_secret(jwt_secret.as_bytes()),
            DecodingKey::from_secret(jwt_secret.as_bytes()),
            db_pool.clone(),
        )
    };

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

    // Spawn the match preview generation background task
    MatchingService::spawn_preview_generation_task(state.db_pool.clone(), &TAG_SYSTEM);

    // Spawn the scheduler background task
    SchedulerService::spawn_scheduler_task(state.db_pool.clone(), &TAG_SYSTEM);

    // Spawn the auto-accept background task
    SchedulerService::spawn_auto_accept_task(state.db_pool.clone());

    let protected_routes = Router::new()
        .route("/api/profile", get(get_profile))
        .route("/api/form", post(submit_form))
        .route("/api/form", get(get_form))
        .route("/api/upload/card", post(upload_card))
        .route("/api/upload/profile-photo", post(upload_profile_photo))
        .route("/api/veto/previews", get(get_previews))
        .route("/api/veto", post(add_veto))
        .route("/api/veto", delete(remove_veto))
        .route("/api/vetoes", get(get_vetoes))
        .route("/api/final-match/accept", post(accept_final_match))
        .route("/api/final-match/reject", post(reject_final_match))
        .route("/api/final-match/time", get(get_next_match_time))
        .route(
            "/api/images/partner/{filename}",
            get(serve_partner_image).route_layer(from_fn_with_state(
                Arc::clone(&state),
                middleware::photo_middleware,
            )),
        )
        .route_layer(from_fn_with_state(
            Arc::clone(&state),
            middleware::auth_middleware,
        ));

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
