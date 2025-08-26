//! # Middleware Components
//!
//! This module contains middleware functions that handle cross-cutting concerns
//! such as authentication, authorization, and request processing.

use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

use crate::{AppState, services::jwt::Claims};

/// Authentication middleware for protecting routes
///
/// This middleware validates JWT access tokens from the Authorization header
/// and extracts user information for use by downstream handlers. Protected
/// routes will automatically receive the authenticated user context.
///
/// # Authentication Flow
///
/// 1. Extracts `Authorization` header with `Bearer <token>` format
/// 2. Validates the JWT token signature and expiration
/// 3. Parses user ID from token claims
/// 4. Adds [`AuthUser`] to request extensions for handler access
///
/// # Returns
///
/// - **Success**: Continues to next handler with user context
/// - **Failure**: Returns `401 Unauthorized` for invalid/missing tokens
///
/// # Usage
///
/// ```rust
/// Router::new()
///     .route("/protected", get(protected_handler))
///     .layer(middleware::from_fn_with_state(state, auth_middleware))
/// ```
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let Some(auth_header) = auth_header else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    if !auth_header.starts_with("Bearer ") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let token = auth_header.trim_start_matches("Bearer ");

    match state.jwt_service.validate_access_token(token) {
        Ok(claims) => {
            let user_id = claims
                .sub
                .parse::<Uuid>()
                .map_err(|_| StatusCode::UNAUTHORIZED)?;

            req.extensions_mut().insert(AuthUser { user_id, claims });
            Ok(next.run(req).await)
        }
        Err(_) => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Authenticated user information available to handlers
///
/// This struct is inserted into request extensions by the authentication
/// middleware and can be extracted by route handlers that need user context.
///
/// # Usage in Handlers
///
/// ```rust
/// async fn protected_handler(Extension(user): Extension<AuthUser>) -> impl IntoResponse {
///     format!("Hello user: {}", user.user_id)
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AuthUser {
    /// Unique identifier for the authenticated user
    pub user_id: Uuid,
    /// JWT claims containing additional token metadata
    pub claims: Claims,
}
