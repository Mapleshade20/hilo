use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct MatchPreview {
    pub id: Uuid,
    pub user_id: Uuid,
    pub candidate_ids: Vec<Uuid>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Veto {
    pub id: Uuid,
    pub vetoer_id: Uuid,
    pub vetoed_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct FinalMatch {
    pub id: Uuid,
    pub user_a_id: Uuid,
    pub user_b_id: Uuid,
    pub score: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct VetoRequest {
    pub vetoed_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProfilePreview {
    pub candidate_id: Uuid,
    pub familiar_tags: Vec<String>,
    pub aspirational_tags: Vec<String>,
    pub recent_topics: String,
    pub email_domain: String,
    pub grade: Option<String>,
}

/// Profile information of the final match partner
///
/// Containing: email domain, grade, familiar tags, aspirational tags,
/// self introduction, and photo URL (if any).
#[derive(Debug, Serialize, Deserialize)]
pub struct FinalPartnerProfile {
    pub email_domain: String,
    pub grade: Option<String>,
    pub familiar_tags: Vec<String>,
    pub aspirational_tags: Vec<String>,
    pub self_intro: String,
    /// Format: /api/images/partner/someuuid.ext
    pub photo_url: Option<String>,
    /// WeChat ID is included only if both users have accepted the match
    pub wechat_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "schedule_status", rename_all = "lowercase")]
pub enum ScheduleStatus {
    Pending,
    Completed,
    Failed,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct ScheduledFinalMatch {
    pub id: Uuid,
    #[serde(with = "time::serde::rfc3339")]
    pub scheduled_time: OffsetDateTime,
    pub status: ScheduleStatus,
    #[serde(with = "time::serde::rfc3339")]
    pub created_at: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub executed_at: Option<OffsetDateTime>,
    pub matches_created: Option<i32>,
    pub error_message: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateScheduledMatchRequest {
    #[serde(with = "time::serde::rfc3339")]
    pub scheduled_time: OffsetDateTime,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateScheduledMatchesRequest {
    pub scheduled_times: Vec<CreateScheduledMatchRequest>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NextMatchTimeResponse {
    #[serde(with = "time::serde::rfc3339::option")]
    pub next: Option<OffsetDateTime>,
}
