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
use tracing::{error, info, instrument};
use uuid::Uuid;

use super::{AdminState, convert_tags_to_stats};
use crate::models::{Form, Gender, UserStatus};
use crate::utils::static_object::UPLOAD_DIR;

/// Pagination query parameters
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
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

/// Admin endpoint to get users overview with pagination
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_users_overview(
    State(state): State<Arc<AdminState>>,
    Query(pagination): Query<PaginationQuery>,
) -> Response {
    // Validate and sanitize pagination parameters
    let limit = pagination.limit.clamp(1, 100); // Max 100, min 1
    let page = pagination.page.max(1);
    let offset = (page - 1) * limit;

    // Get total count
    let total = sqlx::query_scalar!("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db_pool)
        .await;

    let total = match total {
        Ok(Some(count)) => count as u32,
        Ok(None) => 0,
        Err(e) => {
            error!("Failed to get total user count: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Get users with pagination
    let users_result = sqlx::query_as!(
        UserOverview,
        r#"
        SELECT id, email, status as "status: UserStatus"
        FROM users
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#,
        limit as i64,
        offset as i64
    )
    .fetch_all(&state.db_pool)
    .await;

    match users_result {
        Ok(users) => {
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

            (StatusCode::OK, Json(response)).into_response()
        }
        Err(e) => {
            error!("Failed to fetch users: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// Admin endpoint to serve user card photos
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn serve_user_card_photo(
    AxumPath(filename): AxumPath<String>,
    req: Request<Body>,
) -> Response {
    // Construct the file path for card photos
    let file_path = Path::new(UPLOAD_DIR.as_str())
        .join("card_photos")
        .join(&filename);

    info!("Serving card photo: {:?}", file_path);

    // Use tower-http's ServeFile to serve the image
    let mut service = ServeFile::new(file_path);
    match service.try_call(req).await {
        Ok(res) => res.into_response(),
        Err(e) => {
            error!("Failed to serve card photo file: {}", e);
            (StatusCode::NOT_FOUND, "Card photo not found").into_response()
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

/// Admin endpoint to get detailed user information
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_user_detail(
    State(state): State<Arc<AdminState>>,
    AxumPath(user_id): AxumPath<Uuid>,
) -> Response {
    // Get user basic info
    let user_result = sqlx::query!(
        r#"
        SELECT id, email, status as "status: UserStatus", wechat_id,
               grade, card_photo_filename, created_at, updated_at
        FROM users
        WHERE id = $1
        "#,
        user_id
    )
    .fetch_optional(&state.db_pool)
    .await;

    let user = match user_result {
        Ok(Some(user)) => user,
        Ok(None) => {
            return StatusCode::NOT_FOUND.into_response();
        }
        Err(e) => {
            error!("Failed to fetch user: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

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
                .map(|filename| format!("/api/admin/users/{}", filename)),
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
            .map(|filename| format!("/api/admin/users/{}", filename)),
        created_at: user.created_at,
        updated_at: user.updated_at,
        form: form_info,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// Admin endpoint to get tag structure with user counts and IDF scores
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_tags_with_stats(State(state): State<Arc<AdminState>>) -> Response {
    // Get all forms to calculate tag statistics
    let forms = match sqlx::query!("SELECT familiar_tags, aspirational_tags FROM forms")
        .fetch_all(&state.db_pool)
        .await
    {
        Ok(forms) => forms,
        Err(e) => {
            error!("Failed to fetch forms for tag statistics: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
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

    // Load the tag structure from the static TAG_SYSTEM
    let tag_json = match std::fs::read_to_string("tags.json") {
        Ok(content) => content,
        Err(e) => {
            error!("Failed to read tags.json: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let tag_nodes: Vec<crate::models::TagNode> = match serde_json::from_str(&tag_json) {
        Ok(nodes) => nodes,
        Err(e) => {
            error!("Failed to parse tags.json: {}", e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Convert tag nodes to stats format
    let tags_with_stats = convert_tags_to_stats(&tag_nodes, &tag_frequencies, total_user_count);

    (StatusCode::OK, Json(tags_with_stats)).into_response()
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

/// Admin endpoint to get final matches overview with pagination
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_final_matches(
    State(state): State<Arc<AdminState>>,
    Query(pagination): Query<PaginationQuery>,
) -> impl IntoResponse {
    // Validate and sanitize pagination parameters
    let limit = pagination.limit.clamp(1, 100); // Max 100, min 2
    let page = pagination.page.max(1);
    let offset = (page - 1) * limit;

    // Get total count
    let total_count_result = sqlx::query_scalar!("SELECT COUNT(*) FROM final_matches")
        .fetch_one(&state.db_pool)
        .await;

    let total = match total_count_result {
        Ok(Some(count)) => count as u32,
        Ok(None) => 0,
        Err(e) => {
            error!("Failed to get total final matches count: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to get final matches count"
                })),
            )
                .into_response();
        }
    };

    // Get final matches with user emails
    let matches_result = sqlx::query!(
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
    .await;

    match matches_result {
        Ok(matches) => {
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

            (
                StatusCode::OK,
                Json(PaginatedResponse {
                    data: match_overviews,
                    pagination: PaginationInfo {
                        page,
                        limit,
                        total,
                        total_pages,
                    },
                }),
            )
                .into_response()
        }
        Err(e) => {
            error!("Failed to fetch final matches: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to fetch final matches"
                })),
            )
                .into_response()
        }
    }
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

/// Admin endpoint to get user statistics
#[instrument(skip_all, fields(request_id = %uuid::Uuid::new_v4()))]
pub async fn get_user_stats(State(state): State<Arc<AdminState>>) -> impl IntoResponse {
    // Get total user count
    let total_users_result = sqlx::query_scalar!("SELECT COUNT(*) FROM users")
        .fetch_one(&state.db_pool)
        .await;

    let total_users = match total_users_result {
        Ok(Some(count)) => count,
        Ok(None) => 0,
        Err(e) => {
            error!("Failed to get total user count: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to get user statistics"
                })),
            )
                .into_response();
        }
    };

    // Get gender statistics for users with completed forms (form_completed, matched, confirmed statuses)
    let gender_stats_result = sqlx::query!(
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
    .await;

    let mut males = 0i64;
    let mut females = 0i64;

    match gender_stats_result {
        Ok(stats) => {
            for stat in stats {
                match stat.gender {
                    Gender::Male => males = stat.count.unwrap_or(0),
                    Gender::Female => females = stat.count.unwrap_or(0),
                }
            }
        }
        Err(e) => {
            error!("Failed to get gender statistics: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to get user statistics"
                })),
            )
                .into_response();
        }
    };

    // Get unmatched gender statistics (form_completed status only)
    let unmatched_stats_result = sqlx::query!(
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
    .await;

    let mut unmatched_males = 0i64;
    let mut unmatched_females = 0i64;

    match unmatched_stats_result {
        Ok(stats) => {
            for stat in stats {
                match stat.gender {
                    Gender::Male => unmatched_males = stat.count.unwrap_or(0),
                    Gender::Female => unmatched_females = stat.count.unwrap_or(0),
                }
            }
        }
        Err(e) => {
            error!("Failed to get unmatched statistics: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "Failed to get user statistics"
                })),
            )
                .into_response();
        }
    };

    let response = UserStatsResponse {
        total_users,
        males,
        females,
        unmatched_males,
        unmatched_females,
    };

    (StatusCode::OK, Json(response)).into_response()
}
