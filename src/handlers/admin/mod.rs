mod action;
mod view;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
};
use serde::Serialize;
use sqlx::PgPool;
use tracing::warn;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{FinalMatch, Form, TagNode, TagSystem, UserStatus, Veto};
use crate::utils::static_object::TAG_SYSTEM;
use action::{trigger_final_matching, update_match_previews, verify_user};
use view::{
    get_final_matches, get_tags_with_stats, get_user_detail, get_user_stats, get_users_overview,
    serve_user_card_photo,
};

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
        .route("/api/admin/users", get(get_users_overview))
        .route("/api/admin/users/{filename}", get(serve_user_card_photo))
        .route("/api/admin/user/{user_id}", get(get_user_detail))
        .route("/api/admin/tags", get(get_tags_with_stats))
        .route("/api/admin/matches", get(get_final_matches))
        .route("/api/admin/stats", get(get_user_stats))
        .with_state(state)
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

/// Get user ID by email
async fn get_user_id_by_email(db_pool: &PgPool, email: &str) -> AppResult<Uuid> {
    match sqlx::query_scalar!("SELECT id FROM users WHERE email = $1", email)
        .fetch_optional(db_pool)
        .await?
    {
        Some(user_id) => Ok(user_id),
        None => {
            warn!(%email, "User not found");
            Err(AppError::NotFound("User not found"))
        }
    }
}

/// Get current user status
async fn get_user_status(db_pool: &PgPool, user_id: &Uuid) -> AppResult<UserStatus> {
    match sqlx::query!(
        r#"SELECT status as "status: UserStatus" FROM users WHERE id = $1"#,
        user_id
    )
    .fetch_optional(db_pool)
    .await?
    {
        Some(row) => Ok(row.status),
        None => {
            warn!(%user_id, "User not found");
            Err(AppError::NotFound("User not found"))
        }
    }
}

/// Tag with statistics
#[derive(Debug, Serialize)]
pub struct TagWithStats {
    pub id: String,
    pub name: String,
    pub desc: Option<String>,
    pub is_matchable: bool,
    pub user_count: u32,
    pub idf_score: Option<f64>,
    pub children: Option<Vec<TagWithStats>>,
}

/// Convert tag nodes to tags with statistics
fn convert_tags_to_stats(
    nodes: &[TagNode],
    tag_frequencies: &std::collections::HashMap<String, u32>,
    total_user_count: u32,
) -> Vec<TagWithStats> {
    nodes
        .iter()
        .map(|node| {
            let user_count = tag_frequencies.get(&node.id).copied().unwrap_or(0);
            let idf_score = if node.is_matchable && user_count > 0 {
                Some((total_user_count as f64 / user_count as f64).ln())
            } else {
                None
            };

            let children = node.children.as_ref().map(|child_nodes| {
                convert_tags_to_stats(child_nodes, tag_frequencies, total_user_count)
            });

            TagWithStats {
                id: node.id.clone(),
                name: node.name.clone(),
                desc: node.desc.clone(),
                is_matchable: node.is_matchable,
                user_count,
                idf_score,
                children,
            }
        })
        .collect()
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
