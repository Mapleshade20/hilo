//! # Admin Handlers Module
//!
//! This module provides administrative endpoints for the Hilo application.
//! Admin handlers are separated into view endpoints (for retrieving data)
//! and action endpoints (for performing administrative operations).
//!
//! # Available Admin Endpoints
//!
//! ## View Endpoints
//! - **Users Overview** - Paginated list of all users
//! - **User Details** - Detailed information for specific users
//! - **User Card Photos** - Serve student verification card photos
//! - **Tag Statistics** - Tag usage statistics with IDF scores
//! - **Final Matches** - View all final match results
//! - **User Statistics** - Overall user and gender statistics
//!
//! ## Action Endpoints
//! - **Trigger Final Matching** - Execute the final matching algorithm
//! - **Update Match Previews** - Regenerate match preview suggestions
//! - **Verify Users** - Change user verification status
//!
//! # Admin State
//!
//! All admin handlers use a shared `AdminState` containing database pool
//! and tag system references for consistent access to application data.

mod action;
mod view;

use std::sync::Arc;

use axum::{
    Router,
    routing::{delete, get, post},
};
use serde::Serialize;
use sqlx::PgPool;
use tracing::warn;
use uuid::Uuid;

use crate::models::{TagNode, UserStatus};
use crate::{
    error::{AppError, AppResult},
    handlers::admin::view::serve_user_profile_photo,
};
use action::{
    cancel_scheduled_match, create_scheduled_matches, delete_final_match, get_scheduled_matches,
    trigger_final_matching, update_match_previews, verify_user,
};
use view::{
    get_final_matches, get_tags_with_stats, get_user_detail, get_user_stats, get_users_overview,
    serve_user_card_photo,
};

pub struct AdminState {
    pub db_pool: PgPool,
}

/// Create the admin router with admin-specific routes
pub fn admin_router(db_pool: PgPool) -> Router {
    let state = Arc::new(AdminState { db_pool });

    Router::new()
        .route("/api/admin/trigger-match", post(trigger_final_matching))
        .route("/api/admin/update-previews", post(update_match_previews))
        .route("/api/admin/verify-user", post(verify_user))
        .route(
            "/api/admin/scheduled-matches",
            post(create_scheduled_matches),
        )
        .route("/api/admin/scheduled-matches", get(get_scheduled_matches))
        .route(
            "/api/admin/scheduled-matches/{id}",
            delete(cancel_scheduled_match),
        )
        .route("/api/admin/users", get(get_users_overview))
        .route("/api/admin/card/{filename}", get(serve_user_card_photo))
        .route("/api/admin/photo/{filename}", get(serve_user_profile_photo))
        .route("/api/admin/user/{user_id}", get(get_user_detail))
        .route("/api/admin/tags", get(get_tags_with_stats))
        .route("/api/admin/matches", get(get_final_matches))
        .route("/api/admin/final-matches/{id}", delete(delete_final_match))
        .route("/api/admin/stats", get(get_user_stats))
        .with_state(state)
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
