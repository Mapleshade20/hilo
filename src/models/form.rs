use std::collections::HashSet;
use std::env;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize};
use tracing::{error, warn};
use uuid::Uuid;

use crate::handlers::FormRequest;
use crate::models::TagSystem;
use crate::utils::constant::*;

#[derive(Debug, Serialize, Deserialize, sqlx::Type, PartialEq, Eq, Clone, Copy)]
#[sqlx(type_name = "gender", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum Gender {
    Male,
    Female,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Form {
    pub user_id: Uuid,
    pub gender: Gender,
    pub familiar_tags: Vec<String>,
    pub aspirational_tags: Vec<String>,
    pub recent_topics: String,
    pub self_traits: Vec<String>,
    pub ideal_traits: Vec<String>,
    /// Between 1 and 4
    pub physical_boundary: i16,
    pub self_intro: String,
    pub profile_photo_path: Option<String>,
}

impl FormRequest {
    pub fn validate_request(&self, tag_system: &TagSystem) -> Option<Response> {
        // Validate wechat_id
        if self.wechat_id.is_empty() {
            warn!("wechat_id cannot be empty");
            return Some((StatusCode::BAD_REQUEST, "wechat_id cannot be empty").into_response());
        }
        if self.wechat_id.len() > MAX_WECHAT_ID_LENGTH {
            warn!(
                "wechat_id length {} exceeds max {}",
                self.wechat_id.len(),
                MAX_WECHAT_ID_LENGTH
            );
            return Some(
                (
                    StatusCode::BAD_REQUEST,
                    format!("wechat_id cannot exceed {MAX_WECHAT_ID_LENGTH} characters"),
                )
                    .into_response(),
            );
        }

        // Validate total tags limit
        let total_tags = env::var("TOTAL_TAGS")
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or_else(|| {
            error!(
                "Invalid or missing TOTAL_TAGS env var, using fallback value {TOTAL_TAGS_FALLBACK}"
            );
            TOTAL_TAGS_FALLBACK
        });

        let user_total_tags = self.familiar_tags.len() + self.aspirational_tags.len();
        if user_total_tags > total_tags {
            warn!(
                "User submitted {} tags, exceeding limit of {}",
                user_total_tags, total_tags
            );
            return Some(
                (
                    StatusCode::BAD_REQUEST,
                    format!("Total tags cannot exceed {total_tags}"),
                )
                    .into_response(),
            );
        }

        // Validate all tags exist in the tag system
        // Check for duplicate tags within familiar_tags and aspirational_tags
        let mut all_tags = HashSet::new();
        for tag in &self.familiar_tags {
            if !tag_system.is_matchable(tag) {
                warn!("Invalid familiar tag: {}", tag);
                return Some(
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Invalid familiar tag: {tag}"),
                    )
                        .into_response(),
                );
            }
            if !all_tags.insert(tag) {
                warn!("Duplicate tag found in familiar_tags: {}", tag);
                return Some(
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Duplicate tag not allowed: {tag}"),
                    )
                        .into_response(),
                );
            }
        }
        for tag in &self.aspirational_tags {
            if !tag_system.is_matchable(tag) {
                warn!("Invalid aspirational tag: {}", tag);
                return Some(
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Invalid aspirational tag: {tag}"),
                    )
                        .into_response(),
                );
            }
            if !all_tags.insert(tag) {
                warn!("Duplicate tag found in aspirational_tags: {}", tag);
                return Some(
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Duplicate tag not allowed: {tag}"),
                    )
                        .into_response(),
                );
            }
        }

        // Make sure written text is not too long
        if self.recent_topics.len() > MAX_TEXT_FIELD_LENGTH {
            warn!(
                "recent_topics length {} exceeds max {}",
                self.recent_topics.len(),
                MAX_TEXT_FIELD_LENGTH
            );
            return Some(
                (
                    StatusCode::BAD_REQUEST,
                    format!("recent_topics cannot exceed {MAX_TEXT_FIELD_LENGTH} bytes"),
                )
                    .into_response(),
            );
        }
        if self.self_intro.len() > MAX_TEXT_FIELD_LENGTH {
            warn!(
                "self_intro length {} exceeds max {}",
                self.self_intro.len(),
                MAX_TEXT_FIELD_LENGTH
            );
            return Some(
                (
                    StatusCode::BAD_REQUEST,
                    format!("self_intro cannot exceed {MAX_TEXT_FIELD_LENGTH} bytes"),
                )
                    .into_response(),
            );
        }

        // TODO: validate self_traits and ideal_traits tags exist (see traits.json) and each field must contain no more than TOTAL_TRAITS (see .env) tags

        // Validate physical_boundary is between 1 and 4
        if !(1..=4).contains(&self.physical_boundary) {
            warn!(
                "Invalid physical_boundary value: {}",
                self.physical_boundary
            );
            return Some(
                (
                    StatusCode::BAD_REQUEST,
                    "physical_boundary must be between 1 and 3",
                )
                    .into_response(),
            );
        }

        None
    }
}
