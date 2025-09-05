use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use sqlx::PgPool;
use tracing::{debug, error, info, instrument, warn};
use uuid::Uuid;

use crate::middleware::AuthUser;
use crate::models::{AppState, ProfilePreview, Veto, VetoRequest};

/// Get match previews for the authenticated user
#[instrument(
    skip_all,
    fields(
        user_id = %user.user_id,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn get_previews(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<ProfilePreview>>, Response> {
    match fetch_profile_previews(&state.db_pool, user.user_id).await {
        Ok(profiles) => {
            info!("Found {} match previews for user", profiles.len());
            Ok(Json(profiles))
        }
        Err(e) => {
            error!("Failed to fetch match previews for user: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to fetch match previews",
            )
                .into_response())
        }
    }
}

/// Add a veto for a specific user
#[instrument(
    skip_all,
    fields(
        user_id = %user.user_id,
        target_id = %request.vetoed_id,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn add_veto(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(request): Json<VetoRequest>,
) -> Result<StatusCode, Response> {
    let vetoer_id = user.user_id;
    let vetoed_id = request.vetoed_id;

    // Prevent self-vetoing
    if vetoer_id == vetoed_id {
        warn!("User attempted to veto themselves");
        return Err((StatusCode::BAD_REQUEST, "Cannot veto yourself").into_response());
    }

    match create_veto(&state.db_pool, vetoer_id, vetoed_id).await {
        Ok(_) => {
            info!("User successfully vetoed target user");
            Ok(StatusCode::CREATED)
        }
        Err(e) => {
            if e.to_string().contains("duplicate key") {
                debug!("User already vetoed target user");
                Ok(StatusCode::OK) // Idempotent - already vetoed
            } else {
                error!("Failed to create veto: {}", e);
                Err((StatusCode::INTERNAL_SERVER_ERROR, "Failed to create veto").into_response())
            }
        }
    }
}

/// Remove a veto for a specific user
#[instrument(
    skip_all,
    fields(
        user_id = %user.user_id,
        target_id = %request.vetoed_id,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn remove_veto(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(request): Json<VetoRequest>,
) -> Result<StatusCode, Response> {
    let vetoer_id = user.user_id;
    let vetoed_id = request.vetoed_id;

    match delete_veto(&state.db_pool, vetoer_id, vetoed_id).await {
        Ok(rows_affected) => {
            if rows_affected > 0 {
                info!("User successfully removed veto for target user",);
                Ok(StatusCode::NO_CONTENT)
            } else {
                debug!("No veto found to remove between user and target",);
                Ok(StatusCode::NOT_FOUND)
            }
        }
        Err(e) => {
            error!("Failed to remove veto: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Failed to remove veto").into_response())
        }
    }
}

/// Get all vetoes for the authenticated user
#[instrument(
    skip_all,
    fields(
        user_id = %user.user_id,
        request_id = %uuid::Uuid::new_v4()
    )
)]
pub async fn get_vetoes(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
) -> Result<Json<Vec<Uuid>>, Response> {
    match fetch_user_vetoes(&state.db_pool, user.user_id).await {
        Ok(vetoes) => {
            let vetoed_ids: Vec<Uuid> = vetoes.into_iter().map(|v| v.vetoed_id).collect();
            info!("Found {} vetoes for user", vetoed_ids.len());
            Ok(Json(vetoed_ids))
        }
        Err(e) => {
            error!("Failed to fetch vetoes for user: {}", e);
            Err((StatusCode::INTERNAL_SERVER_ERROR, "Failed to fetch vetoes").into_response())
        }
    }
}

// --- Database helper functions ---

async fn create_veto(
    db_pool: &PgPool,
    vetoer_id: Uuid,
    vetoed_id: Uuid,
) -> Result<Veto, sqlx::Error> {
    sqlx::query_as!(
        Veto,
        "INSERT INTO vetoes (vetoer_id, vetoed_id) VALUES ($1, $2)
         RETURNING id, vetoer_id, vetoed_id",
        vetoer_id,
        vetoed_id
    )
    .fetch_one(db_pool)
    .await
}

async fn delete_veto(
    db_pool: &PgPool,
    vetoer_id: Uuid,
    vetoed_id: Uuid,
) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        "DELETE FROM vetoes WHERE vetoer_id = $1 AND vetoed_id = $2",
        vetoer_id,
        vetoed_id
    )
    .execute(db_pool)
    .await?;

    Ok(result.rows_affected())
}

async fn fetch_user_vetoes(db_pool: &PgPool, user_id: Uuid) -> Result<Vec<Veto>, sqlx::Error> {
    sqlx::query_as!(
        Veto,
        "SELECT id, vetoer_id, vetoed_id FROM vetoes WHERE vetoer_id = $1",
        user_id
    )
    .fetch_all(db_pool)
    .await
}

async fn fetch_profile_previews(
    db_pool: &PgPool,
    user_id: Uuid,
) -> Result<Vec<ProfilePreview>, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        SELECT
            f.familiar_tags,
            f.aspirational_tags,
            f.recent_topics,
            u.email,
            u.grade
        FROM match_previews mp
        JOIN forms f ON f.user_id = ANY(mp.candidate_ids)
        JOIN users u ON u.id = f.user_id
        WHERE mp.user_id = $1
        "#,
        user_id
    )
    .fetch_all(db_pool)
    .await?;

    let profiles = result
        .into_iter()
        .map(|row| {
            let email_domain = row.email.split('@').nth(1).unwrap_or("").to_string();
            ProfilePreview {
                familiar_tags: row.familiar_tags,
                aspirational_tags: row.aspirational_tags,
                recent_topics: row.recent_topics,
                email_domain,
                grade: row.grade,
            }
        })
        .collect();

    Ok(profiles)
}
