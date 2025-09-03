//! # Health Check Handler
//!
//! Simple health check endpoint for monitoring application availability.
//! This endpoint can be used by load balancers, monitoring systems, or
//! deployment tools to verify that the application is running.

use axum::http::StatusCode;
use tracing::{debug, instrument};

/// Health check endpoint that returns 200 OK.
///
/// GET /health-check
#[instrument]
pub async fn health_check() -> StatusCode {
    debug!("Health check endpoint accessed");
    StatusCode::OK
}
