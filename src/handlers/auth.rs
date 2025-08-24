use std::time::Instant;

use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use validator::Validate;

use crate::AppState;
use crate::utils::{
    constant::{EMAIL_RATE_LIMIT, VERIFICATION_CODE_EXPIRY},
    validator::EMAIL_REGEX,
};

#[derive(Debug, Deserialize, Validate)]
pub struct SendCodeRequest {
    #[validate(regex(path = "*EMAIL_REGEX"))]
    pub email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct VerifyRequest {
    #[validate(regex(path = "*EMAIL_REGEX"))]
    pub email: String,
    #[validate(length(equal = 6))]
    pub code: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

#[derive(Debug, Deserialize, Validate)]
pub struct RefreshTokenRequest {
    pub refresh_token: String,
}

pub async fn send_verification_code(
    State(state): State<AppState>,
    Json(payload): Json<SendCodeRequest>,
) -> impl IntoResponse {
    // 1. Validate format
    if let Err(e) = payload.validate() {
        return (StatusCode::BAD_REQUEST, format!("Invalid input: {e}")).into_response();
    }

    // 2. Check rate limit
    if let Some(entry) = state.rate_limit_cache.get(&payload.email)
        && entry.elapsed() < EMAIL_RATE_LIMIT
    {
        let remaining = EMAIL_RATE_LIMIT - entry.elapsed();
        let message = format!(
            "Rate limit exceeded. Try again in {} seconds.",
            remaining.as_secs()
        );
        return (StatusCode::TOO_MANY_REQUESTS, message).into_response();
    }

    // 3. Generate verification code
    let code = format!("{:06}", rand::random::<u32>() % 1_000_000);

    // 4. Cache code and timestamp
    state
        .verification_code_cache
        .insert(payload.email.clone(), (code.clone(), Instant::now()));
    state
        .rate_limit_cache
        .insert(payload.email.clone(), Instant::now());

    // 5. Send email
    match state
        .email_service
        .send_email(
            &payload.email,
            "Your verification code",
            &format!("Your verification code is: {code}"), // TODO: write good html
        )
        .await
    {
        Err(e) => {
            error!("Failed to send verification code: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to send email").into_response()
        }
        Ok(_) => {
            info!("Sent verification code to {}", payload.email);
            (StatusCode::OK, "Verification code sent").into_response()
        }
    }
}

pub async fn verify_code(
    State(state): State<AppState>,
    Json(payload): Json<VerifyRequest>,
) -> impl IntoResponse {
    // 1. Validate format
    if let Err(e) = payload.validate() {
        return (StatusCode::BAD_REQUEST, format!("Invalid input: {e}")).into_response();
    }

    // 2. Check verification code (do not leak references into the map)
    let is_valid = state
        .verification_code_cache
        .get(&payload.email)
        .map(|entry| {
            let (cached_code, created_at) = entry.value();
            cached_code == &payload.code && created_at.elapsed() <= VERIFICATION_CODE_EXPIRY
        })
        .unwrap_or(false);
    if !is_valid {
        return (StatusCode::BAD_REQUEST, "Invalid or expired code").into_response();
    }

    // 3. Insert user in DB
    let Ok(user_id) = sqlx::query_scalar!(
        r#"
        INSERT INTO users (email, status) VALUES ($1, 'verified')
        ON CONFLICT (email) DO UPDATE SET status = 'verified'
        RETURNING id
        "#,
        payload.email
    )
    .fetch_one(state.db_pool.as_ref())
    .await
    else {
        error!("Database error when inserting user");
        return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response();
    };

    // 4. Generate JWT token pair
    let token_pair = match state.jwt_service.create_token_pair(user_id).await {
        Ok(pair) => pair,
        Err(e) => {
            error!("Failed to create token pair: {e}");
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to create tokens").into_response();
        }
    };

    // 5. Remove verification code from cache
    state.verification_code_cache.remove(&payload.email);
    // if cache reachs const CODE_CACHE_CAPACITY, remove invalid entries
    // do this in background task

    (
        StatusCode::OK,
        Json(AuthResponse {
            access_token: token_pair.access_token,
            refresh_token: token_pair.refresh_token,
            token_type: "Bearer".to_string(),
            expires_in: token_pair.expires_in,
        }),
    )
        .into_response()
}

pub async fn refresh_token(
    State(state): State<AppState>,
    Json(payload): Json<RefreshTokenRequest>,
) -> impl IntoResponse {
    match state
        .jwt_service
        .refresh_token_pair(&payload.refresh_token)
        .await
    {
        Ok(token_pair) => (
            StatusCode::OK,
            Json(AuthResponse {
                access_token: token_pair.access_token,
                refresh_token: token_pair.refresh_token,
                token_type: "Bearer".to_string(),
                expires_in: token_pair.expires_in,
            }),
        )
            .into_response(),
        Err(_) => (StatusCode::UNAUTHORIZED, "Invalid refresh token").into_response(),
    }
}
