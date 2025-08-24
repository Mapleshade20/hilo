pub mod handlers;
pub mod middleware;
pub mod services;
pub mod utils;

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
use crate::services::email::{EmailService, LogEmailer, MailgunEmailer};
use crate::services::jwt::JwtService;

/// Thread-safe application state shared across requests. (Arc wrapped)
#[derive(Clone)]
pub struct AppState {
    /// A map of email addresses to their rate limit timestamps.
    pub rate_limit_cache: Arc<DashMap<String, Instant>>,
    /// A map of email addresses to their verification codes and timestamps.
    pub verification_code_cache: Arc<DashMap<String, (String, Instant)>>,
    /// The email service used to send verification codes.
    pub email_service: Arc<dyn EmailService + Send + Sync>,
    /// The PostgreSQL database connection pool.
    pub db_pool: Arc<PgPool>,
    /// JWT service for token generation and validation.
    pub jwt_service: Arc<JwtService>,
}

pub fn app(db_pool: PgPool) -> Router {
    app_with_email_service(db_pool, None)
}

pub fn app_with_email_service(
    db_pool: PgPool,
    email_service: Option<Arc<dyn EmailService + Send + Sync>>,
) -> Router {
    let db_pool = Arc::new(db_pool);

    let email_service: Arc<dyn EmailService + Send + Sync> = if let Some(service) = email_service {
        service
    } else {
        let app_env = env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());

        if app_env == "production" {
            info!("Running in production mode with [MailgunEmailer]");
            let api_key =
                env::var("MAILGUN_API_KEY").expect("Env variable `MAILGUN_API_KEY` should be set");
            let sender =
                env::var("SENDER_EMAIL").expect("Env variable `SENDER_EMAIL` should be set");
            Arc::new(MailgunEmailer::new(api_key, sender))
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
        db_pool.clone(),
    );

    let state = AppState {
        rate_limit_cache: Arc::new(DashMap::new()),
        verification_code_cache: Arc::new(DashMap::new()),
        email_service,
        db_pool,
        jwt_service: Arc::new(jwt_service),
    };

    Router::new()
        .route("/health-check", get(health_check))
        .route("/api/auth/send-code", post(send_verification_code))
        .route("/api/auth/verify-code", post(verify_code))
        .route("/api/auth/refresh", post(refresh_token))
        .with_state(state)
}
