//! # Authentication Handlers
//!
//! This module implements HTTP handlers for user authentication using email verification
//! and JWT tokens. The authentication flow consists of:
//!
//! 1. Sending a verification code to the user's email
//! 2. Verifying the code and creating a user account
//! 3. Issuing JWT access and refresh tokens
//! 4. Refreshing tokens when needed
//!
//! The email endpoint includes rate limiting and input validation for security.

use std::{borrow::Cow, sync::Arc, time::Instant};

use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, instrument, trace, warn};
use validator::Validate;

use crate::{
    error::{AppError, AppResult},
    models::AppState,
    utils::{constant::*, static_object::EMAIL_REGEX},
};

/// Request payload for sending verification code to email
#[derive(Debug, Deserialize, Validate)]
pub struct SendCodeRequest {
    #[validate(regex(path = "*EMAIL_REGEX"))]
    pub email: String,
}

/// Request payload for verifying email with code
#[derive(Debug, Deserialize, Validate)]
pub struct VerifyRequest {
    #[validate(regex(path = "*EMAIL_REGEX"))]
    pub email: String,
    #[validate(length(equal = 6))]
    pub code: String,
}

/// Response containing JWT tokens after successful authentication
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: Cow<'static, str>,
    pub expires_in: u64,
}

/// Request payload for refreshing JWT tokens
#[derive(Debug, Deserialize, Validate)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

/// Sends a verification code to the specified email address.
///
/// POST /api/auth/send-code email=
///
/// # Rate Limiting
///
/// Users can only request a verification code once per [`EMAIL_RATE_LIMIT`] duration.
///
/// # Returns
///
/// - `202 Accepted` - Verification code sent successfully
/// - `400 Bad Request` - Invalid email format
/// - `429 Too Many Requests` - Rate limit exceeded
/// - `500 Internal Server Error` - Email service failure
#[instrument(
    skip_all,
    fields(
        email = %payload.email,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn send_verification_code(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<SendCodeRequest>,
) -> AppResult<impl IntoResponse> {
    debug!("Processing verification code request");

    // Validate format
    if payload.validate().is_err() {
        warn!("Invalid email format provided");
        return Err(AppError::BadRequest("Invalid input"));
    }

    // Check rate limit
    if let Some(entry) = state.rate_limit_cache.get(&payload.email)
        && entry.elapsed() < EMAIL_RATE_LIMIT
    {
        let remaining = EMAIL_RATE_LIMIT - entry.elapsed();
        warn!(
            remaining_seconds = remaining.as_secs(),
            "Rate limit exceeded for email"
        );
        return Err(AppError::TooManyRequests);
    }

    // Generate verification code
    let code = format!("{:06}", rand::rng().random_range(0..1_000_000));
    debug!("Generated verification code");

    // Cache code and timestamp
    state
        .verification_code_cache
        .insert(payload.email.clone(), (code.clone(), Instant::now()));
    state
        .rate_limit_cache
        .insert(payload.email.clone(), Instant::now());
    debug!("Cached verification code and rate limit");

    // Send email
    state
        .email_service
        .send_email(&payload.email, "Your login code to Project Contigo", &code)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to send verification code");
            AppError::Internal
        })?;

    info!("Successfully sent verification code");
    Ok((StatusCode::ACCEPTED, "Verification code sent"))
}

/// Verifies the email verification code and creates user account.
///
/// POST /api/auth/verify-code email= code=
///
/// This endpoint validates the verification code sent to the user's email,
/// creates or updates the user account in the database, and issues JWT tokens
/// for authentication.
///
/// # Security
///
/// - Codes expire after [`VERIFICATION_CODE_EXPIRY`] duration
/// - Codes are removed from cache after successful verification
/// - User accounts are created with 'unverified' status
///
/// # Returns
///
/// - `200 OK` with `AuthResponse` - Code correct, returns JWT tokens
/// - `400 Bad Request` - Invalid input or expired/invalid code
/// - `500 Internal Server Error` - Database or token generation failure
#[instrument(
    skip_all,
    fields(
        email = %payload.email,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn verify_code(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<VerifyRequest>,
) -> AppResult<impl IntoResponse> {
    debug!("Processing code verification request");

    // Validate format
    if payload.validate().is_err() {
        warn!("Invalid verification request format");
        return Err(AppError::BadRequest("Invalid input"));
    }

    // Check verification code (do not leak references into the map)
    let is_valid = state
        .verification_code_cache
        .get(&payload.email)
        .map(|entry| {
            let (cached_code, created_at) = entry.value();
            let is_code_match = cached_code == &payload.code;
            let is_not_expired = created_at.elapsed() <= VERIFICATION_CODE_EXPIRY;
            debug!(
                code_match = is_code_match,
                expired = !is_not_expired,
                "Code validation result"
            );
            is_code_match && is_not_expired
        })
        .unwrap_or(false);

    if !is_valid {
        warn!("Invalid or expired verification code provided");
        return Err(AppError::BadRequest("Invalid or expired code"));
    }

    // Insert user in DB
    trace!("Creating/updating user in database");
    let user_id = sqlx::query_scalar!(
        r#"
        INSERT INTO users (email, status) VALUES ($1, 'unverified')
        ON CONFLICT (email) DO UPDATE SET email = EXCLUDED.email
        RETURNING id
        "#,
        payload.email
    )
    .fetch_one(&state.db_pool)
    .await?;

    debug!(user_id = %user_id, "User created/updated successfully");

    // Generate JWT token pair
    trace!("Generating JWT token pair");
    let token_pair = state
        .jwt_service
        .create_token_pair(user_id)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to create token pair");
            AppError::Internal
        })?;

    debug!("JWT token pair created successfully");

    // Remove verification code from cache
    state.verification_code_cache.remove(&payload.email);
    debug!("Verification code removed from cache");
    // if cache reachs const CACHE_CAPACITY, remove invalid entries
    // do this in background task

    info!("Code verification completed successfully");
    Ok((
        StatusCode::OK,
        Json(AuthResponse {
            access_token: token_pair.access_token,
            refresh_token: token_pair.refresh_token,
            token_type: "Bearer".into(),
            expires_in: token_pair.expires_in,
        }),
    ))
}

/// Refreshes JWT token pair using a valid refresh token.
///
/// POST /api/auth/refresh refresh_token=
///
/// This endpoint allows clients to obtain new access and refresh tokens
/// using a valid refresh token, extending the user's session without
/// requiring re-authentication.
///
/// # Security
///
/// - Refresh tokens are validated against the database
/// - Old refresh tokens are invalidated when new ones are issued
/// - Invalid refresh tokens result in unauthorized response
///
/// # Returns
///
/// - `200 OK` with `AuthResponse` - New token pair issued successfully
/// - `401 Unauthorized` - Invalid or expired refresh token
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn refresh_token(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<RefreshTokenRequest>,
) -> AppResult<impl IntoResponse> {
    debug!("Processing token refresh request");

    let token_pair = state
        .jwt_service
        .refresh_token_pair(&payload.refresh_token)
        .await
        .map_err(|e| {
            warn!(error = %e, "Token refresh failed");
            AppError::Unauthorized("Invalid refresh token")
        })?;

    info!("Token refresh successful");
    Ok((
        StatusCode::OK,
        Json(AuthResponse {
            access_token: token_pair.access_token,
            refresh_token: token_pair.refresh_token,
            token_type: "Bearer".into(),
            expires_in: token_pair.expires_in,
        }),
    ))
}
