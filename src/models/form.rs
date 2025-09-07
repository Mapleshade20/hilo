use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tracing::warn;
use uuid::Uuid;

use crate::handlers::FormRequest;
use crate::models::TagSystem;
use crate::utils::{
    constant::*,
    static_object::{TAGS_LIMIT_SUM, TRAITS, TRAITS_LIMIT_EACH},
};

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
    pub profile_photo_filename: Option<String>,
}

impl FormRequest {
    pub fn validate_request(&self, tag_system: &TagSystem) -> Result<(), &'static str> {
        // Validate wechat_id
        if self.wechat_id.is_empty() {
            warn!("wechat_id cannot be empty");
            return Err("wechat_id cannot be empty");
        }
        if self.wechat_id.len() > MAX_WECHAT_ID_LENGTH {
            warn!(
                "wechat_id length {} exceeds max {}",
                self.wechat_id.len(),
                MAX_WECHAT_ID_LENGTH
            );
            return Err("wechat_id too long");
        }

        // Validate total tags limit
        let tags_limit_sum = *TAGS_LIMIT_SUM;

        let user_total_tags = self.familiar_tags.len() + self.aspirational_tags.len();
        if user_total_tags > tags_limit_sum {
            warn!(
                "User submitted {} tags, exceeding limit of {}",
                user_total_tags, tags_limit_sum
            );
            return Err("Total tags exceed limit");
        }

        // Validate all tags exist in the tag system
        // Check for duplicate tags within familiar_tags and aspirational_tags
        let mut all_tags = HashSet::new();
        for tag in &self.familiar_tags {
            if !tag_system.is_matchable(tag) {
                warn!("Invalid familiar tag: {}", tag);
                return Err("Invalid familiar tag");
            }
            if !all_tags.insert(tag) {
                warn!("Duplicate tag found in familiar_tags: {}", tag);
                return Err("Duplicate tag not allowed");
            }
        }
        for tag in &self.aspirational_tags {
            if !tag_system.is_matchable(tag) {
                warn!("Invalid aspirational tag: {}", tag);
                return Err("Invalid aspirational tag");
            }
            if !all_tags.insert(tag) {
                warn!("Duplicate tag found in aspirational_tags: {}", tag);
                return Err("Duplicate tag not allowed");
            }
        }

        // Make sure written text is not too long
        if self.recent_topics.len() > MAX_TEXT_FIELD_LENGTH {
            warn!(
                "recent_topics length {} exceeds max {}",
                self.recent_topics.len(),
                MAX_TEXT_FIELD_LENGTH
            );
            return Err("recent_topics too long");
        }
        if self.self_intro.len() > MAX_TEXT_FIELD_LENGTH {
            warn!(
                "self_intro length {} exceeds max {}",
                self.self_intro.len(),
                MAX_TEXT_FIELD_LENGTH
            );
            return Err("self_intro too long");
        }

        // Validate self_traits and ideal_traits tags exist and limits
        let traits_limit = *TRAITS_LIMIT_EACH;

        if self.self_traits.len() > traits_limit {
            warn!(
                "self_traits count {} exceeds limit of {}",
                self.self_traits.len(),
                traits_limit
            );
            return Err("Too many self traits");
        }

        if self.ideal_traits.len() > traits_limit {
            warn!(
                "ideal_traits count {} exceeds limit of {}",
                self.ideal_traits.len(),
                traits_limit
            );
            return Err("Too many ideal traits");
        }

        let mut self_traits_set = HashSet::new();
        for trait_id in &self.self_traits {
            if !TRAITS.contains(trait_id) {
                warn!("Invalid self trait: {}", trait_id);
                return Err("Invalid self trait");
            }
            if !self_traits_set.insert(trait_id) {
                warn!("Duplicate trait found in self_traits: {}", trait_id);
                return Err("Duplicate self trait not allowed");
            }
        }

        let mut ideal_traits_set = HashSet::new();
        for trait_id in &self.ideal_traits {
            if !TRAITS.contains(trait_id) {
                warn!("Invalid ideal trait: {}", trait_id);
                return Err("Invalid ideal trait");
            }
            if !ideal_traits_set.insert(trait_id) {
                warn!("Duplicate trait found in ideal_traits: {}", trait_id);
                return Err("Duplicate ideal trait not allowed");
            }
        }

        // Validate physical_boundary is between 1 and 4
        if !(1..=4).contains(&self.physical_boundary) {
            warn!(
                "Invalid physical_boundary value: {}",
                self.physical_boundary
            );
            return Err("physical_boundary must be between 1 and 3");
        }

        Ok(())
    }
}
