//! # Admin View Handlers
//!
//! This module implements read-only administrative endpoints for viewing
//! application data. These endpoints provide paginated access to user information,
//! detailed user profiles, tag statistics, final match results, and overall
//! system statistics.
//!
//! # Security
//!
//! All endpoints in this module are admin-only and should be protected
//! by appropriate authentication middleware in the router configuration.
//!
//! # Pagination
//!
//! List endpoints support pagination with configurable page size (1-100 items)
//! and include pagination metadata in responses.

use std::collections::HashMap;
use std::sync::Arc;

use axum::http::Request;
use axum::response::Response;
use axum::{
    Json,
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::StatusCode,
    response::IntoResponse,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use time::OffsetDateTime;
use tower_http::services::ServeFile;
use tracing::{debug, error, instrument};
use uuid::Uuid;

use super::{AdminState, convert_tags_to_stats};
use crate::error::{AppError, AppResult};
use crate::models::{Form, Gender, UserStatus};
use crate::utils::static_object::{TAG_TREE, UPLOAD_DIR};

/// Pagination query parameters
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
    pub status: Option<UserStatus>,
}
fn default_page() -> u32 {
    1
}
fn default_limit() -> u32 {
    20
}

/// User overview response item
#[derive(Debug, Serialize)]
pub struct UserOverview {
    pub id: Uuid,
    pub email: String,
    pub status: UserStatus,
    pub wechat_id: Option<String>,
}

/// Paginated response wrapper
#[derive(Debug, Serialize)]
pub struct PaginatedResponse<T> {
    pub data: Vec<T>,
    pub pagination: PaginationInfo,
}

/// Pagination metadata
#[derive(Debug, Serialize)]
pub struct PaginationInfo {
    pub page: u32,
    pub limit: u32,
    pub total: u32,
    pub total_pages: u32,
}

/// Gets a paginated overview of all users in the system.
///
/// GET /api/admin/users ?page=1&limit=20&status=verification_pending
///
/// This endpoint returns a paginated list of users with basic information
/// (ID, email, status). Results are ordered by creation date (newest first).
/// Supports pagination with configurable page size (1-100 items per page).
/// Optionally filters users by status when the status query parameter is provided.
///
/// # Query Parameters
///
/// - `page`: Page number (default: 1)
/// - `limit`: Items per page (default: 20, max: 100)
/// - `status`: Optional status filter (e.g., "verification_pending")
///
/// # Returns
///
/// - `200 OK` with `PaginatedResponse<UserOverview>` - Users retrieved successfully
/// - `500 Internal Server Error` - Database error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_users_overview(
    State(state): State<Arc<AdminState>>,
    Query(pagination): Query<PaginationQuery>,
) -> AppResult<impl IntoResponse> {
    // Validate and sanitize pagination parameters
    let limit = pagination.limit.clamp(1, 100); // Max 100, min 1
    let page = pagination.page.max(1);
    let offset = (page - 1) * limit;

    // Get total count
    let total = if let Some(status) = &pagination.status {
        sqlx::query_scalar!(
            "SELECT COUNT(*) FROM users WHERE status = $1",
            *status as UserStatus
        )
        .fetch_one(&state.db_pool)
        .await?
        .unwrap_or(0) as u32
    } else {
        sqlx::query_scalar!("SELECT COUNT(*) FROM users")
            .fetch_one(&state.db_pool)
            .await?
            .unwrap_or(0) as u32
    };

    // Get users with pagination
    let users = if let Some(status) = &pagination.status {
        sqlx::query_as!(
            UserOverview,
            r#"
            SELECT id, email, status as "status: UserStatus", wechat_id
            FROM users
            WHERE status = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#,
            *status as UserStatus,
            limit as i64,
            offset as i64
        )
        .fetch_all(&state.db_pool)
        .await?
    } else {
        sqlx::query_as!(
            UserOverview,
            r#"
            SELECT id, email, status as "status: UserStatus", wechat_id
            FROM users
            ORDER BY created_at DESC
            LIMIT $1 OFFSET $2
            "#,
            limit as i64,
            offset as i64
        )
        .fetch_all(&state.db_pool)
        .await?
    };

    let total_pages = total.div_ceil(limit); // Ceiling division
    let response = PaginatedResponse {
        data: users,
        pagination: PaginationInfo {
            page,
            limit,
            total,
            total_pages,
        },
    };

    Ok(Json(response))
}

/// Serves student card photos for admin review.
///
/// GET /api/admin/card/{filename}
///
/// This endpoint serves student verification card photos stored in the filesystem.
/// Used by admins to review submitted cards during the user verification process.
///
/// # Returns
///
/// - `200 OK` with image file - Card photo served successfully
/// - `404 Not Found` - Card photo file not found
/// - `500 Internal Server Error` - File system error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn serve_user_card_photo(
    AxumPath(filename): AxumPath<String>,
    req: Request<Body>,
) -> Response {
    // Construct the file path for card photos
    let file_path = Path::new(UPLOAD_DIR.as_str())
        .join("card_photos")
        .join(&filename);

    // Use tower-http's ServeFile to serve the image
    let mut service = ServeFile::new(file_path);
    match service.try_call(req).await {
        Ok(res) => {
            if res.status() == StatusCode::OK {
                debug!(%filename, "Card photo served successfully");
            } else {
                error!(%filename, "Card photo not found");
            }
            res.into_response()
        }
        Err(e) => {
            error!("Failed to serve card photo file: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Serves user profile photos for admin review.
///
/// GET /api/admin/photo/{filename}
///
/// This endpoint serves user profile photos stored in the filesystem.
///
/// # Returns
///
/// - `200 OK` with image file - Profile photo served successfully
/// - `404 Not Found` - Profile photo file not found
/// - `500 Internal Server Error` - File system error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn serve_user_profile_photo(
    AxumPath(filename): AxumPath<String>,
    req: Request<Body>,
) -> Response {
    // Construct the file path for profile photos
    let file_path = Path::new(UPLOAD_DIR.as_str())
        .join("profile_photos")
        .join(&filename);

    // Use tower-http's ServeFile to serve the image
    let mut service = ServeFile::new(file_path);
    match service.try_call(req).await {
        Ok(res) => {
            if res.status() == StatusCode::OK {
                debug!(%filename, "Profile photo served successfully");
            } else {
                error!(%filename, "Profile photo not found");
            }
            res.into_response()
        }
        Err(e) => {
            error!("Failed to serve profile photo file: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Failed to serve file").into_response()
        }
    }
}

/// Detailed user info response
#[derive(Debug, Serialize)]
pub struct UserDetailResponse {
    // Basic user info
    pub id: Uuid,
    pub email: String,
    pub status: UserStatus,
    pub wechat_id: Option<String>,
    pub grade: Option<String>,
    pub card_photo_uri: Option<String>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    // Form info (if exists)
    pub form: Option<UserFormInfo>,
}

/// User form information
#[derive(Debug, Serialize)]
pub struct UserFormInfo {
    pub gender: Gender,
    pub familiar_tags: Vec<String>,
    pub aspirational_tags: Vec<String>,
    pub recent_topics: String,
    pub self_traits: Vec<String>,
    pub ideal_traits: Vec<String>,
    pub physical_boundary: i16,
    pub self_intro: String,
    pub profile_photo_uri: Option<String>,
}

/// Gets detailed information for a specific user.
///
/// GET /api/admin/user/{user_id}
///
/// This endpoint returns comprehensive user information including basic profile data,
/// form responses (if submitted), and links to uploaded files. Used by admins for
/// detailed user review and verification processes.
///
/// # Returns
///
/// - `200 OK` with `UserDetailResponse` - User details retrieved successfully
/// - `404 Not Found` - User not found
/// - `500 Internal Server Error` - Database error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_user_detail(
    State(state): State<Arc<AdminState>>,
    AxumPath(user_id): AxumPath<Uuid>,
) -> AppResult<impl IntoResponse> {
    // Get user basic info
    let user = sqlx::query!(
        r#"
        SELECT id, email, status as "status: UserStatus", wechat_id,
               grade, card_photo_filename, created_at, updated_at
        FROM users
        WHERE id = $1
        "#,
        user_id
    )
    .fetch_optional(&state.db_pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found"))?;

    // Get user form info if exists
    let form_result = sqlx::query_as!(
        Form,
        r#"
        SELECT user_id, gender as "gender: Gender", familiar_tags, aspirational_tags,
               recent_topics, self_traits, ideal_traits, physical_boundary,
               self_intro, profile_photo_filename
        FROM forms
        WHERE user_id = $1
        "#,
        user_id
    )
    .fetch_optional(&state.db_pool)
    .await;

    let form_info = match form_result {
        Ok(Some(form)) => Some(UserFormInfo {
            gender: form.gender,
            familiar_tags: form.familiar_tags,
            aspirational_tags: form.aspirational_tags,
            recent_topics: form.recent_topics,
            self_traits: form.self_traits,
            ideal_traits: form.ideal_traits,
            physical_boundary: form.physical_boundary,
            self_intro: form.self_intro,
            profile_photo_uri: form
                .profile_photo_filename
                .map(|filename| format!("/api/admin/photo/{}", filename)),
        }),
        Ok(None) => None,
        Err(e) => {
            error!("Failed to fetch user form: {}", e);
            None
        }
    };

    let response = UserDetailResponse {
        id: user.id,
        email: user.email,
        status: user.status,
        wechat_id: user.wechat_id,
        grade: user.grade,
        card_photo_uri: user
            .card_photo_filename
            .map(|filename| format!("/api/admin/card/{}", filename)),
        created_at: user.created_at,
        updated_at: user.updated_at,
        form: form_info,
    };

    Ok(Json(response))
}

/// Gets the tag system structure with usage statistics.
///
/// GET /api/admin/tags
///
/// This endpoint returns the complete tag hierarchy with user count and IDF
/// (Inverse Document Frequency) scores for each tag. Used by admins to understand
/// tag usage patterns and matching algorithm behavior.
///
/// # Returns
///
/// - `200 OK` with `Vec<TagWithStats>` - Tag statistics retrieved successfully
/// - `500 Internal Server Error` - Database error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_tags_with_stats(
    State(state): State<Arc<AdminState>>,
) -> AppResult<impl IntoResponse> {
    // Get all forms to calculate tag statistics
    let forms = sqlx::query!("SELECT familiar_tags, aspirational_tags FROM forms")
        .fetch_all(&state.db_pool)
        .await?;

    let total_user_count = forms.len() as u32;

    // Calculate tag frequencies for IDF scoring
    let mut tag_frequencies: HashMap<String, u32> = HashMap::new();
    for tag in forms.into_iter().flat_map(|rec| {
        rec.familiar_tags
            .into_iter()
            .chain(rec.aspirational_tags.into_iter())
    }) {
        *tag_frequencies.entry(tag).or_insert(0) += 1;
    }

    // Convert tag nodes to stats format
    let tags_with_stats = convert_tags_to_stats(&TAG_TREE, &tag_frequencies, total_user_count);

    Ok(Json(tags_with_stats))
}

/// Final match overview item
#[derive(Debug, Serialize)]
pub struct FinalMatchOverview {
    pub id: Uuid,
    pub user_a_id: Uuid,
    pub user_a_email: String,
    pub user_b_id: Uuid,
    pub user_b_email: String,
    pub score: f64,
}

/// Gets a paginated overview of all final matches.
///
/// GET /api/admin/matches ?page=1&limit=20
///
/// This endpoint returns a paginated list of final matches created by the matching
/// algorithm, including match scores and participant email addresses. Results are
/// ordered by match score (highest first). Used by admins to review match quality.
///
/// # Returns
///
/// - `200 OK` with `PaginatedResponse<FinalMatchOverview>` - Final matches retrieved successfully
/// - `500 Internal Server Error` - Database error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_final_matches(
    State(state): State<Arc<AdminState>>,
    Query(pagination): Query<PaginationQuery>,
) -> AppResult<impl IntoResponse> {
    // Validate and sanitize pagination parameters
    let limit = pagination.limit.clamp(1, 100); // Max 100, min 2
    let page = pagination.page.max(1);
    let offset = (page - 1) * limit;

    // Get total count
    let total = sqlx::query_scalar!("SELECT COUNT(*) FROM final_matches")
        .fetch_one(&state.db_pool)
        .await?
        .unwrap_or(0) as u32;

    // Get final matches with user emails
    let matches = sqlx::query!(
        r#"
        SELECT
            fm.id,
            fm.user_a_id,
            fm.user_b_id,
            fm.score,
            ua.email as user_a_email,
            ub.email as user_b_email
        FROM final_matches fm
        JOIN users ua ON fm.user_a_id = ua.id
        JOIN users ub ON fm.user_b_id = ub.id
        ORDER BY fm.score DESC
        LIMIT $1 OFFSET $2
        "#,
        limit as i64,
        offset as i64
    )
    .fetch_all(&state.db_pool)
    .await?;

    let match_overviews: Vec<FinalMatchOverview> = matches
        .into_iter()
        .map(|row| FinalMatchOverview {
            id: row.id,
            user_a_id: row.user_a_id,
            user_a_email: row.user_a_email,
            user_b_id: row.user_b_id,
            user_b_email: row.user_b_email,
            score: row.score,
        })
        .collect();

    let total_pages = total.div_ceil(limit);

    Ok(Json(PaginatedResponse {
        data: match_overviews,
        pagination: PaginationInfo {
            page,
            limit,
            total,
            total_pages,
        },
    }))
}

/// User statistics response
#[derive(Debug, Serialize)]
pub struct UserStatsResponse {
    pub total_users: i64,
    pub males: i64,
    pub females: i64,
    pub unmatched_males: i64,
    pub unmatched_females: i64,
}

/// Gets overall user and gender statistics.
///
/// GET /api/admin/stats
///
/// This endpoint returns aggregate statistics about users, including total counts,
/// gender distribution among users with completed forms, and unmatched user counts
/// by gender. Used by admins for system monitoring and matching insights.
///
/// # Returns
///
/// - `200 OK` with `UserStatsResponse` - User statistics retrieved successfully
/// - `500 Internal Server Error` - Database error
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_user_stats(State(state): State<Arc<AdminState>>) -> AppResult<impl IntoResponse> {
    // Get total user count
    let total_users = sqlx::query_scalar!("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db_pool)
        .await?
        .unwrap_or(0);

    // Get gender statistics for users with completed forms (form_completed, matched, confirmed statuses)
    let gender_stats = sqlx::query!(
        r#"
        SELECT
            f.gender as "gender: Gender",
            COUNT(*) as count
        FROM forms f
        JOIN users u ON f.user_id = u.id
        WHERE u.status IN ('form_completed', 'matched', 'confirmed')
        GROUP BY f.gender
        "#
    )
    .fetch_all(&state.db_pool)
    .await?;

    let mut males = 0i64;
    let mut females = 0i64;

    for stat in gender_stats {
        match stat.gender {
            Gender::Male => males = stat.count.unwrap_or(0),
            Gender::Female => females = stat.count.unwrap_or(0),
        }
    }

    // Get unmatched gender statistics (form_completed status only)
    let unmatched_stats = sqlx::query!(
        r#"
        SELECT
            f.gender as "gender: Gender",
            COUNT(*) as count
        FROM forms f
        JOIN users u ON f.user_id = u.id
        WHERE u.status = 'form_completed'
        GROUP BY f.gender
        "#
    )
    .fetch_all(&state.db_pool)
    .await?;

    let mut unmatched_males = 0i64;
    let mut unmatched_females = 0i64;

    for stat in unmatched_stats {
        match stat.gender {
            Gender::Male => unmatched_males = stat.count.unwrap_or(0),
            Gender::Female => unmatched_females = stat.count.unwrap_or(0),
        }
    }

    let response = UserStatsResponse {
        total_users,
        males,
        females,
        unmatched_males,
        unmatched_females,
    };

    Ok(Json(response))
}
