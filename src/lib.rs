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
use crate::utils::constant::{EMAIL_RATE_LIMIT, VERIFICATION_CODE_EXPIRY};

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

    pub fn cleanup_expired_entries(&self) {
        // Clean verification codes
        self.verification_code_cache
            .retain(|_, (_, timestamp)| timestamp.elapsed() <= VERIFICATION_CODE_EXPIRY);

        // Clean rate limits
        self.rate_limit_cache
            .retain(|_, timestamp| timestamp.elapsed() <= EMAIL_RATE_LIMIT);
    }
}

pub fn app(db_pool: PgPool) -> Router {
    app_with_email_service(db_pool, None)
}

pub fn app_with_email_service(
    db_pool: PgPool,
    email_service: Option<Arc<dyn EmailService>>,
) -> Router {
    let email_service: Arc<dyn EmailService> = if let Some(service) = email_service {
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
    );

    let state = Arc::new(AppState::new(email_service, db_pool, jwt_service));

    // TODO: a cleanup routine job goes here

    Router::new()
        .route("/health-check", get(health_check))
        .route("/api/auth/send-code", post(send_verification_code))
        .route("/api/auth/verify-code", post(verify_code))
        .route("/api/auth/refresh", post(refresh_token))
        .with_state(state)
}
