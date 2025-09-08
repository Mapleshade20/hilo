//! # Centralized Error Handling
//!
//! This module provides a unified error handling system for the application.
//! It centralizes error logging and HTTP response generation, eliminating
//! repetitive error handling patterns throughout the codebase.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use thiserror::Error;
use tracing::error;

/// Central application error type that encompasses all possible error conditions.
///
/// This enum provides a unified way to handle errors across the application,
/// with automatic conversion to appropriate HTTP responses. _Db errors are logged
/// automatically, while other errors should be logged at the point of creation if needed._
#[derive(Error, Debug)]
pub enum AppError {
    #[error("database error")]
    Db(#[from] sqlx::Error),

    #[error("UUID parsing error")]
    Uuid(#[from] uuid::Error),

    #[error("not found: {0}")]
    NotFound(&'static str),

    #[error("bad request: {0}")]
    BadRequest(&'static str),

    #[error("forbidden: {0}")]
    Forbidden(&'static str),

    #[error("internal server error")]
    Internal,

    #[error("unauthorized: {0}")]
    Unauthorized(&'static str),

    #[error("too many requests")]
    TooManyRequests,
}

#[derive(Serialize)]
struct ErrorBody {
    message: &'static str,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        if let AppError::Db(e) = &self {
            // Log detailed database errors for internal tracking
            error!(?e, "Database error occurred");
        }

        // Central logging - log details for internal errors, minimal for client errors
        let (status, message) = match self {
            AppError::Db(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Database error"),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            AppError::Forbidden(msg) => (StatusCode::FORBIDDEN, msg),
            AppError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error"),
            AppError::Unauthorized(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded"),
            AppError::Uuid(_) => (StatusCode::BAD_REQUEST, "Invalid UUID format"),
        };

        let body = Json(ErrorBody { message });
        (status, body).into_response()
    }
}

/// Convenience Result type alias that uses AppError as the error type.
pub type AppResult<T> = Result<T, AppError>;
