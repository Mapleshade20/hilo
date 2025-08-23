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

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub token: String,
}

pub async fn send_verification_code(
    State(state): State<AppState>,
    Json(payload): Json<SendCodeRequest>,
) -> impl IntoResponse {
    // 1. Validate format
    if let Err(e) = payload.validate() {
        return (StatusCode::BAD_REQUEST, format!("Invalid input: {}", e)).into_response();
    }

    // 2. Check rate limit
    if let Some(entry) = state.rate_limit_cache.get(&payload.email) {
        if entry.elapsed() < EMAIL_RATE_LIMIT {
            let remaining = EMAIL_RATE_LIMIT - entry.elapsed();
            let message = format!(
                "Rate limit exceeded. Try again in {} seconds.",
                remaining.as_secs()
            );
            return (StatusCode::TOO_MANY_REQUESTS, message).into_response();
        }
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
            &format!("Your verification code is: {}", code), // TODO: write good html
        )
        .await
    {
        Err(e) => {
            error!("Failed to send verification code: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to send email").into_response()
        }
        Ok(_) => {
            info!("Sent verification code to {}", payload.email);
            (StatusCode::OK, "Verification code sent").into_response()
        }
    }
}

// pub async fn verify_code(
//     State(state): State<Arc<AppState>>,
//     Json(payload): Json<VerifyRequest>,
// ) -> impl IntoResponse {
//     // 1. Validate format
//     if let Err(e) = payload.validate() {
//         return (StatusCode::BAD_REQUEST, format!("Invalid input: {}", e)).into_response();
//     }

//     // 2. Check verification code
//     let Some(entry) = state.verification_code_cache.get(&payload.email) else {
//         return (StatusCode::BAD_REQUEST, "Invalid or expired code").into_response();
//     };
//     let (cached_code, created_at) = entry.value();
//     if cached_code != &payload.code || created_at.elapsed() > VERIFICATION_CODE_EXPIRY {
//         return (StatusCode::BAD_REQUEST, "Invalid or expired code").into_response();
//     }

//     // 3. 数据库操作 (假设db_pool在AppState中)
//     let user_id = sqlx::query_scalar!(
//         r#"
//         INSERT INTO users (email, status) VALUES ($1, 'verified')
//         ON CONFLICT (email) DO UPDATE SET status = 'verified'
//         RETURNING id
//         "#,
//         payload.email
//     )
//     .fetch_one(&state.db_pool)
//     .await
//     .unwrap(); // 错误处理

//     // 4. 生成JWT
//     // let claims = Claims { sub: user_id.to_string(), exp: ... };
//     // let secret = "YOUR_JWT_SECRET"; // 从配置读取
//     // let token = encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_ref())).unwrap();

//     // 成功后，从缓存中移除验证码
//     // state.verification_code_cache.remove(&payload.email);

//     // return (StatusCode::OK, Json(AuthResponse { token })).into_response();

//     (
//         StatusCode::OK,
//         Json(AuthResponse {
//             token: "fake-jwt-token".to_string(),
//         }),
//     )
//         .into_response() // 模拟成功
// }
