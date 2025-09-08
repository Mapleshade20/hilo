//! # Authentication Middleware
//!
//! This module contains the authentication middleware that validates JWT tokens
//! and provides user context to protected routes.

use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use tracing::{debug, error, instrument, trace, warn};
use uuid::Uuid;

use crate::models::AppState;
use crate::services::jwt::Claims;

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
#[instrument(
    skip_all,
    fields(
        method = %req.method(),
        uri = %req.uri(),
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    trace!("Processing authentication middleware");

    let auth_header = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok());

    let Some(auth_header) = auth_header else {
        warn!("Missing Authorization header");
        return Err(StatusCode::UNAUTHORIZED);
    };

    if !auth_header.starts_with("Bearer ") {
        warn!("Invalid Authorization header format");
        return Err(StatusCode::UNAUTHORIZED);
    }

    let token = auth_header.trim_start_matches("Bearer ");
    trace!("Extracted bearer token from Authorization header");

    match state.jwt_service.validate_access_token(token) {
        Ok(claims) => {
            let user_id = uuid::Uuid::try_parse(&claims.sub).map_err(|e| {
                error!(error = %e, "Failed to parse user ID from token claims");
                StatusCode::UNAUTHORIZED
            })?;

            debug!(user_id = %user_id, "Authentication successful");
            req.extensions_mut().insert(AuthUser { user_id, claims });

            let response = next.run(req).await;
            debug!("Request processed successfully");
            Ok(response)
        }
        Err(e) => {
            warn!(error = %e, "Token validation failed");
            Err(StatusCode::UNAUTHORIZED)
        }
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
/// use axum::{extract::Extension, response::IntoResponse};
/// use hilo::middleware::AuthUser;
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
