pub mod handlers;
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
use tracing::info;

use crate::handlers::{health_check, send_verification_code};
use crate::services::email::{EmailService, LogEmailer, MailgunEmailer};

/// Thread-safe application state shared across requests. (Arc wrapped)
#[derive(Clone)]
pub struct AppState {
    /// A map of email addresses to their rate limit timestamps.
    pub rate_limit_cache: Arc<DashMap<String, Instant>>,
    /// A map of email addresses to their verification codes and timestamps.
    pub verification_code_cache: Arc<DashMap<String, (String, Instant)>>,
    /// The email service used to send verification codes.
    pub email_service: Arc<dyn EmailService + Send + Sync>,
}

pub fn app() -> Router {
    app_with_email_service(None)
}

pub fn app_with_email_service(
    email_service: Option<Arc<dyn EmailService + Send + Sync>>,
) -> Router {
    let email_service: Arc<dyn EmailService + Send + Sync> = if let Some(service) = email_service {
        service
    } else {
        let app_env = env::var("APP_ENV").unwrap_or_else(|_| "development".to_string());

        if app_env == "production" {
            info!("Running in production mode with [MailgunEmailer]");
            let api_key = env::var("MAILGUN_API_KEY").expect("Missing environment variable");
            let sender = env::var("SENDER_EMAIL").expect("Missing environment variable");
            Arc::new(MailgunEmailer::new(api_key, sender))
        } else {
            info!("Running in development mode with [LogEmailer (Mock)]");
            Arc::new(LogEmailer)
        }
    };

    let state = AppState {
        rate_limit_cache: Arc::new(DashMap::new()),
        verification_code_cache: Arc::new(DashMap::new()),
        email_service,
    };

    Router::new()
        .route("/health-check", get(health_check))
        .route("/api/auth/send-code", post(send_verification_code))
        .with_state(state)
}
