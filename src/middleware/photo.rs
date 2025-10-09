//! # Partner Validation Middleware
//!
//! This middleware validates that a user requesting a partner's image
//! is actually matched with that partner in the final_matches table.

use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use tracing::{debug, instrument, trace, warn};
use uuid::Uuid;

use crate::{middleware::AuthUser, models::AppState};

/// Partner validation middleware for image access
///
/// This middleware checks if the authenticated user is matched with the
/// requested partner (by UUID) in the `final_matches` table. If they are
/// matched, the request proceeds; otherwise, a `403 Forbidden` is returned.
/// The requested partner ID is extracted from the request path.
/// If validation is successful, the partner ID is inserted into the request
/// extensions for downstream handlers to use.
///
/// # Returns
///
/// - **Success**: Continues to next handler with partner ID in extensions
/// - **Failure**: Returns `403 Forbidden` if not matched, or `500 Internal Server Error` on DB issues
#[instrument(skip_all, fields(from = %user.user_id, requested = %filename))]
pub async fn photo_middleware(
    State(state): State<Arc<AppState>>,
    Path(filename): Path<String>,
    Extension(user): Extension<AuthUser>,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    trace!("Validating image access");

    let Some(requested_id) = filename
        .split('.')
        .next()
        .and_then(|s| Uuid::try_parse(s).ok())
    else {
        warn!("Invalid filename format");
        return Err(StatusCode::BAD_REQUEST);
    };

    let exists = sqlx::query_scalar!(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM final_matches
            WHERE (user_a_id = $1 AND user_b_id = $2)
               OR (user_a_id = $2 AND user_b_id = $1)
        )
        "#,
        user.user_id,
        requested_id
    )
    .fetch_one(&state.db_pool)
    .await;

    match exists {
        Ok(Some(true)) => {
            debug!("Image access validation successful");
            req.extensions_mut().insert(requested_id);

            let response = next.run(req).await;
            Ok(response)
        }
        Ok(_) => {
            warn!("Image access denied");
            Err(StatusCode::FORBIDDEN)
        }
        Err(e) => {
            warn!("Database error during partner validation: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
