use serde::{Deserialize, Serialize};
use sqlx::FromRow;
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
    pub familiar_tags: Vec<String>,
    pub aspirational_tags: Vec<String>,
    pub recent_topics: String,
    pub email_domain: String,
    pub grade: Option<String>,
}
