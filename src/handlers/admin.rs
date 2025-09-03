use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::PgPool;
use tracing::{error, info, instrument, warn};
use uuid::Uuid;
use validator::Validate;

use crate::models::{FinalMatch, Form, Gender, TagSystem, UserStatus, Veto};
use crate::services::matching::MatchingService;
use crate::utils::static_object::{EMAIL_REGEX, TAG_SYSTEM};

/// Request payload for admin user verification
#[derive(Debug, Deserialize, Validate)]
pub struct VerifyUserRequest {
    /// User ID (takes priority if both id and email are provided)
    pub user_id: Option<Uuid>,
    /// User email (used if user_id is not provided)
    #[validate(regex(path = "*EMAIL_REGEX"))]
    pub email: Option<String>,
    /// Target verification status (verified or unverified)
    pub status: UserStatus,
}

/// Response for admin user verification
#[derive(Debug, Serialize)]
pub struct VerifyUserResponse {
    pub success: bool,
    pub message: String,
    pub user_id: Uuid,
    pub new_status: UserStatus,
}

pub struct AdminState {
    pub db_pool: PgPool,
    pub tag_system: &'static TagSystem,
}

/// Create the admin router with admin-specific routes
pub fn admin_router(db_pool: PgPool) -> Router {
    let state = Arc::new(AdminState {
        db_pool,
        tag_system: &TAG_SYSTEM,
    });

    Router::new()
        .route("/api/admin/trigger-match", post(trigger_final_matching))
        .route("/api/admin/update-previews", post(update_match_previews))
        .route("/api/admin/verify-user", post(verify_user))
        .with_state(state)
}

/// Admin endpoint to trigger the final matching algorithm
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn trigger_final_matching(
    State(state): State<Arc<AdminState>>,
) -> Result<Json<serde_json::Value>, Response> {
    match execute_final_matching(&state.db_pool, state.tag_system).await {
        Ok(matches) => {
            info!("Final matching completed: {} pairs", matches.len());
            Ok(Json(json!({
                "success": true,
                "message": format!("Successfully created {} final matches", matches.len()),
                "match_count": matches.len()
            })))
        }
        Err(e) => {
            error!("Final matching failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "error": "Failed to execute final matching"
                })),
            )
                .into_response())
        }
    }
}

/// Admin endpoint to manually update match previews
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn update_match_previews(
    State(state): State<Arc<AdminState>>,
) -> Result<Json<serde_json::Value>, Response> {
    match MatchingService::generate_match_previews(&state.db_pool, state.tag_system).await {
        Ok(_) => {
            info!("Match previews update completed successfully");
            Ok(Json(json!({
                "success": true,
                "message": "Match previews updated successfully"
            })))
        }
        Err(e) => {
            error!("Match previews update failed: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "error": "Failed to update match previews"
                })),
            )
                .into_response())
        }
    }
}

/// Admin endpoint to set user verification status
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn verify_user(
    State(state): State<Arc<AdminState>>,
    Json(payload): Json<VerifyUserRequest>,
) -> Result<Json<VerifyUserResponse>, Response> {
    // 1. Validate input
    if payload.validate().is_err() {
        warn!("Invalid verify user request format");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": "Invalid input format"
            })),
        )
            .into_response());
    }

    // 2. Validate target status
    if !matches!(
        payload.status,
        UserStatus::Verified | UserStatus::Unverified
    ) {
        warn!("Invalid target status: {:?}", payload.status);
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": "Target status must be either 'verified' or 'unverified'"
            })),
        )
            .into_response());
    }

    // 3. Get user ID (prioritize user_id over email)
    let user_id = if let Some(id) = payload.user_id {
        id
    } else if let Some(email) = &payload.email {
        match get_user_id_by_email(&state.db_pool, email).await {
            Ok(id) => id,
            Err(e) => {
                warn!("User not found by email: {}", email);
                return Err(e);
            }
        }
    } else {
        warn!("Neither user_id nor email provided");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": "Either user_id or email must be provided"
            })),
        )
            .into_response());
    };

    // 4. Check current user status: should not be 'unverified'
    let current_status = match get_user_status(&state.db_pool, &user_id).await {
        Ok(status) => status,
        Err(e) => {
            error!("Failed to get user status for user_id: {}", user_id);
            return Err(e);
        }
    };

    if current_status == UserStatus::Unverified {
        warn!(
            "User {} status is {:?}, expected verification_pending",
            user_id, current_status
        );
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({
                "success": false,
                "error": format!("User status is '{}', expected 'verification_pending'", current_status)
            })),
        )
            .into_response());
    }

    // 5. Update user status
    match update_user_status(&state.db_pool, &user_id, payload.status).await {
        Ok(_) => {
            info!(
                "Successfully updated user {} status from {:?} to {:?}",
                user_id, current_status, payload.status
            );
            Ok(Json(VerifyUserResponse {
                success: true,
                message: format!("User status updated to '{}'", payload.status),
                user_id,
                new_status: payload.status,
            }))
        }
        Err(e) => {
            error!("Failed to update user status: {}", e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({
                    "success": false,
                    "error": "Failed to update user status"
                })),
            )
                .into_response())
        }
    }
}

/// Execute the final matching algorithm using greedy approach
async fn execute_final_matching(
    db_pool: &PgPool,
    tag_system: &TagSystem,
) -> Result<Vec<FinalMatch>, Box<dyn std::error::Error + Send + Sync>> {
    // 1. Fetch all users with submitted forms
    let forms = fetch_all_forms(db_pool).await?;
    if forms.is_empty() {
        return Ok(vec![]);
    }

    // 2. Fetch all veto records
    let vetoes = fetch_all_vetoes(db_pool).await?;
    let veto_map = build_veto_map(&vetoes);

    // 3. Calculate tag frequencies for IDF scoring
    let tag_frequencies = calculate_tag_frequencies(&forms);
    let total_user_count = forms.len() as u32;

    // 4. Build score matrix for all valid pairs
    let mut pair_scores = Vec::new();

    for (i, form_a) in forms.iter().enumerate() {
        for (j, form_b) in forms.iter().enumerate() {
            if i >= j {
                continue; // Only consider each pair once
            }

            let score = MatchingService::calculate_match_score(
                form_a,
                form_b,
                tag_system,
                &tag_frequencies,
                total_user_count,
            );

            // Apply vetoes - if either user has vetoed the other, set score to -1
            if is_vetoed(form_a.user_id, form_b.user_id, &veto_map)
                || is_vetoed(form_b.user_id, form_a.user_id, &veto_map)
            {
                continue; // Skip vetoed pairs entirely
            }

            if score > 0.0 {
                pair_scores.push((form_a.user_id, form_b.user_id, score));
            }
        }
    }

    // 5. Sort by score (descending) for greedy algorithm
    pair_scores.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    // 6. Greedy matching algorithm
    let mut matched_users = HashSet::new();
    let mut final_matches = Vec::new();

    for (user_a, user_b, score) in pair_scores {
        if !matched_users.contains(&user_a) && !matched_users.contains(&user_b) {
            // Create the final match
            let final_match = create_final_match(db_pool, user_a, user_b, score).await?;
            final_matches.push(final_match);

            matched_users.insert(user_a);
            matched_users.insert(user_b);
        }
    }

    Ok(final_matches)
}

/// Check if user_a has vetoed user_b
fn is_vetoed(user_a: Uuid, user_b: Uuid, veto_map: &HashMap<Uuid, HashSet<Uuid>>) -> bool {
    veto_map
        .get(&user_a)
        .is_some_and(|vetoed_set| vetoed_set.contains(&user_b))
}

/// Build a map of vetoer_id -> set of vetoed_ids for efficient lookup
fn build_veto_map(vetoes: &[Veto]) -> HashMap<Uuid, HashSet<Uuid>> {
    let mut veto_map = HashMap::new();

    for veto in vetoes {
        veto_map
            .entry(veto.vetoer_id)
            .or_insert_with(HashSet::new)
            .insert(veto.vetoed_id);
    }

    veto_map
}

/// Calculate tag frequencies for IDF scoring - same as in preview generation
fn calculate_tag_frequencies(forms: &[Form]) -> HashMap<String, u32> {
    let mut frequencies = HashMap::new();

    for form in forms {
        for tag in &form.familiar_tags {
            *frequencies.entry(tag.clone()).or_insert(0) += 1;
        }
        for tag in &form.aspirational_tags {
            *frequencies.entry(tag.clone()).or_insert(0) += 1;
        }
    }

    frequencies
}

// Database helper functions

async fn fetch_all_forms(db_pool: &PgPool) -> Result<Vec<Form>, sqlx::Error> {
    sqlx::query_as!(
        Form,
        r#"
        SELECT user_id, gender as "gender: Gender", familiar_tags, aspirational_tags, recent_topics, 
               self_traits, ideal_traits, physical_boundary, self_intro, profile_photo_path
        FROM forms
        "#,
    )
    .fetch_all(db_pool)
    .await
}

async fn fetch_all_vetoes(db_pool: &PgPool) -> Result<Vec<Veto>, sqlx::Error> {
    sqlx::query_as!(Veto, "SELECT id, vetoer_id, vetoed_id FROM vetoes")
        .fetch_all(db_pool)
        .await
}

async fn create_final_match(
    db_pool: &PgPool,
    user_a_id: Uuid,
    user_b_id: Uuid,
    score: f64,
) -> Result<FinalMatch, sqlx::Error> {
    // Ensure consistent ordering: smaller UUID first
    let (first_user, second_user) = if user_a_id < user_b_id {
        (user_a_id, user_b_id)
    } else {
        (user_b_id, user_a_id)
    };

    sqlx::query_as!(
        FinalMatch,
        r#"
        INSERT INTO final_matches (user_a_id, user_b_id, score)
        VALUES ($1, $2, $3)
        RETURNING id, user_a_id, user_b_id, score
        "#,
        first_user,
        second_user,
        score
    )
    .fetch_one(db_pool)
    .await
}

// Helper functions for user verification

/// Get user ID by email
async fn get_user_id_by_email(db_pool: &PgPool, email: &str) -> Result<Uuid, Response> {
    match sqlx::query_scalar!("SELECT id FROM users WHERE email = $1", email)
        .fetch_optional(db_pool)
        .await
    {
        Ok(Some(user_id)) => Ok(user_id),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error": "User not found"
            })),
        )
            .into_response()),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": "Database error"
            })),
        )
            .into_response()),
    }
}

/// Get current user status
async fn get_user_status(db_pool: &PgPool, user_id: &Uuid) -> Result<UserStatus, Response> {
    match sqlx::query!(
        r#"SELECT status as "status: UserStatus" FROM users WHERE id = $1"#,
        user_id
    )
    .fetch_optional(db_pool)
    .await
    {
        Ok(Some(row)) => Ok(row.status),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(json!({
                "success": false,
                "error": "User not found"
            })),
        )
            .into_response()),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "success": false,
                "error": "Database error"
            })),
        )
            .into_response()),
    }
}

/// Update user status
async fn update_user_status(
    db_pool: &PgPool,
    user_id: &Uuid,
    new_status: UserStatus,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE users SET status = $1 WHERE id = $2"#,
        new_status as UserStatus,
        user_id
    )
    .execute(db_pool)
    .await?;

    Ok(())
}
