use std::collections::{HashMap, HashSet};

use sqlx::PgPool;
use tracing::{debug, instrument, trace};
use uuid::Uuid;

use crate::error::AppResult;
use crate::models::{Form, Gender, TagSystem};
use crate::utils::{
    constant::{IDF_MIN, MATCH_PREVIEW_INTERVAL},
    static_object::{
        BOUNDARY_MATCH_POINTS, COMPLEMENTARY_TAG_WEIGHT, MAX_PREVIEW_CANDIDATES,
        TAG_SCORE_DECAY_FACTOR, TRAIT_MATCH_POINTS,
    },
};

pub struct MatchingService;

static INCOMPATIBLE_MATCH_SCORE: f64 = -1.0;

impl MatchingService {
    /// Calculates the compatibility score between two users
    /// Returns INCOMPATIBLE_MATCH_SCORE for impossible matches, positive scores for viable matches
    pub fn calculate_match_score(
        form_a: &Form,
        form_b: &Form,
        tag_system: &TagSystem,
        tag_frequencies: &HashMap<String, u32>,
        total_user_count: u32,
    ) -> f64 {
        // Hard Filters (Dealbreakers)

        // Gender Filter: Must be one male and one female
        if !Self::is_gender_compatible(form_a.gender, form_b.gender) {
            return INCOMPATIBLE_MATCH_SCORE;
        }

        // Physical Boundary Filter: Difference must be <= 1
        let boundary_diff =
            Self::physical_boundary_difference(form_a.physical_boundary, form_b.physical_boundary);
        if boundary_diff > 1 {
            trace!(
                "Physical boundary incompatible: {} and {}",
                form_a.physical_boundary, form_b.physical_boundary
            );
            return INCOMPATIBLE_MATCH_SCORE;
        }

        // Scored Components (Points-based)
        let mut score = 0.0;

        // A. Hierarchical Tag Scoring (Most Important Component)
        let complementary_weight = *COMPLEMENTARY_TAG_WEIGHT;

        // Familiar x Familiar (high weight)
        score += Self::calculate_tag_set_score(
            &form_a.familiar_tags,
            &form_b.familiar_tags,
            tag_system,
            tag_frequencies,
            total_user_count,
        ) * 1.0;

        // Familiar x Aspirational (cross-matching)
        score += Self::calculate_tag_set_score(
            &form_a.familiar_tags,
            &form_b.aspirational_tags,
            tag_system,
            tag_frequencies,
            total_user_count,
        ) * complementary_weight;

        score += Self::calculate_tag_set_score(
            &form_b.familiar_tags,
            &form_a.aspirational_tags,
            tag_system,
            tag_frequencies,
            total_user_count,
        ) * complementary_weight;

        // B. Personal Traits Scoring (B4)
        score += Self::calculate_trait_compatibility(form_a, form_b);

        // C. Physical Boundary Scoring (B5)
        // Equal boundary get a small bonus
        if boundary_diff == 0 {
            score += *BOUNDARY_MATCH_POINTS;
        }

        trace!(
            user_a = %form_a.user_id, user_b = %form_b.user_id,
            "Match score calculated: {}", score
        );

        score
    }

    /// Check if two genders are compatible (one male, one female)
    fn is_gender_compatible(gender_a: Gender, gender_b: Gender) -> bool {
        matches!(
            (gender_a, gender_b),
            (Gender::Male, Gender::Female) | (Gender::Female, Gender::Male)
        )
    }

    /// Calculate the absolute difference between two physical boundary values
    fn physical_boundary_difference(boundary_a: i16, boundary_b: i16) -> i16 {
        (boundary_a - boundary_b).abs()
    }

    /// Calculate compatibility score for a pair of tag sets using hierarchical matching
    pub fn calculate_tag_set_score(
        tags_a: &[String],
        tags_b: &[String],
        tag_system: &TagSystem,
        tag_frequencies: &HashMap<String, u32>,
        total_user_count: u32,
    ) -> f64 {
        let mut score = 0.0;

        // Convert to HashSets for efficient operations
        let set_a: HashSet<&String> = tags_a.iter().collect();
        let set_b: HashSet<&String> = tags_b.iter().collect();

        // Direct matches (exact tag matches)
        let direct_matches: HashSet<_> = set_a.intersection(&set_b).collect();
        for tag in &direct_matches {
            score += Self::calculate_idf_score(tag, tag_frequencies, total_user_count);
            trace!(
                "Direct tag match: {} (IDF: {})",
                tag,
                Self::calculate_idf_score(tag, tag_frequencies, total_user_count)
            );
        }

        // Indirect matches (common ancestors)
        let decay_factor = *TAG_SCORE_DECAY_FACTOR;
        // Avoid double-counting indirect matches via the same ancestor
        let mut matched_ancestors = HashSet::new();

        for tag_a in tags_a {
            for tag_b in tags_b {
                // Skip if this was a direct match
                if tag_a == tag_b && direct_matches.contains(&tag_a) {
                    continue;
                }

                let Some(common_ancestor) =
                    Self::find_closest_common_ancestor(tag_a, tag_b, tag_system)
                else {
                    continue;
                };

                if tag_system.is_matchable(&common_ancestor)
                    && !matched_ancestors.contains(&common_ancestor)
                {
                    let ancestor_score = Self::calculate_idf_score(
                        &common_ancestor,
                        tag_frequencies,
                        total_user_count,
                    );
                    score += ancestor_score * decay_factor;

                    trace!(
                        "Indirect tag match: {} <-> {} via {} (IDF: {}, decayed: {})",
                        tag_a,
                        tag_b,
                        common_ancestor,
                        ancestor_score,
                        ancestor_score * decay_factor
                    );

                    matched_ancestors.insert(common_ancestor);
                }
            }
        }

        score
    }

    /// Find the closest common ancestor between two tags
    fn find_closest_common_ancestor(
        tag_a: &str,
        tag_b: &str,
        tag_system: &TagSystem,
    ) -> Option<String> {
        let ancestors_a = tag_system.get_all_ancestors(tag_a);
        let ancestors_b = tag_system.get_all_ancestors(tag_b);

        // Check each ancestor of tag_a to see if it's also an ancestor of tag_b
        // Since get_all_ancestors returns ancestors in order from immediate parent to root,
        // the first match will be the closest common ancestor
        for ancestor_a in &ancestors_a {
            if ancestors_b.contains(ancestor_a) {
                return Some(ancestor_a.clone());
            }
        }

        None
    }

    /// Calculate IDF (Inverse Document Frequency) score for a tag
    fn calculate_idf_score(
        tag: &str,
        tag_frequencies: &HashMap<String, u32>,
        total_user_count: u32,
    ) -> f64 {
        let frequency = tag_frequencies.get(tag).copied().unwrap_or(1);
        let idf = (total_user_count as f64 / frequency as f64).log2();

        // Ensure we don't get negative or zero scores
        idf.max(IDF_MIN)
    }

    /// Calculate trait compatibility score between two users
    fn calculate_trait_compatibility(form_a: &Form, form_b: &Form) -> f64 {
        let set_a_desired: HashSet<&String> = form_a.ideal_traits.iter().collect();
        let set_b_self: HashSet<&String> = form_b.self_traits.iter().collect();
        let set_b_desired: HashSet<&String> = form_b.ideal_traits.iter().collect();
        let set_a_self: HashSet<&String> = form_a.self_traits.iter().collect();

        // Count how many of A's desired traits are in B's self traits
        let a_satisfied = set_a_desired.intersection(&set_b_self).count();

        // Count how many of B's desired traits are in A's self traits
        let b_satisfied = set_b_desired.intersection(&set_a_self).count();

        let trait_match_points = *TRAIT_MATCH_POINTS;
        let total_matches = (a_satisfied + b_satisfied) as f64;

        total_matches * trait_match_points
    }

    /// Generate match previews for all users and store them in the database
    #[instrument(skip_all, err)]
    pub async fn generate_match_previews(
        db_pool: &PgPool,
        tag_system: &TagSystem,
    ) -> AppResult<()> {
        let forms = Self::fetch_unmatched_forms(db_pool).await?;
        if forms.is_empty() {
            debug!("No forms found, skipping match preview generation");
            return Ok(());
        }

        // Fetch all existing vetoes
        let veto_map = Self::build_map_vetoed_as_key(db_pool).await?;

        // Calculate tag frequencies for IDF scoring
        let tag_frequencies = Self::calculate_tag_frequencies(&forms, tag_system);
        let total_user_count = forms.len() as u32;

        // Generate previews for each user
        for (i, user_form) in forms.iter().enumerate() {
            let mut candidate_scores = Vec::new();

            // Score against all other users
            for (j, candidate_form) in forms.iter().enumerate() {
                if i == j {
                    continue; // Skip self
                }

                // Skip if candidate has vetoed this user
                if let Some(vetoers) = veto_map.get(&user_form.user_id)
                    && vetoers.contains(&candidate_form.user_id)
                {
                    continue;
                }

                let score = Self::calculate_match_score(
                    user_form,
                    candidate_form,
                    tag_system,
                    &tag_frequencies,
                    total_user_count,
                );

                if score > 0.0 {
                    candidate_scores.push((candidate_form.user_id, score));
                }
            }

            // Sort by score (descending) and take top N candidates
            candidate_scores
                .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let (top_candidates, top_scores): (Vec<_>, Vec<_>) = candidate_scores
                .into_iter()
                .take(*MAX_PREVIEW_CANDIDATES)
                .unzip();

            // Store in database using UPSERT
            Self::store_match_preview(db_pool, user_form.user_id, &top_candidates, &top_scores)
                .await?;

            trace!(
                user_id = %user_form.user_id,
                "Generated {} match previews for user, scores: {:?}",
                top_candidates.len(), top_scores
            );
        }

        debug!(user_count = %total_user_count, "Preview generation completed");
        Ok(())
    }

    /// Calculate tag frequencies across all forms for IDF scoring
    /// Counts both leaf tags and all their ancestors to ensure realistic IDF scores
    pub(crate) fn calculate_tag_frequencies(
        forms: &[Form],
        tag_system: &TagSystem,
    ) -> HashMap<String, u32> {
        let mut frequencies = HashMap::new();

        for form in forms {
            // Count familiar tags and their ancestors
            for tag in &form.familiar_tags {
                // Count the tag itself
                *frequencies.entry(tag.clone()).or_insert(0) += 1;

                // Count all ancestors
                for ancestor in tag_system.get_all_ancestors(tag) {
                    *frequencies.entry(ancestor).or_insert(0) += 1;
                }
            }

            // Count aspirational tags and their ancestors
            for tag in &form.aspirational_tags {
                // Count the tag itself
                *frequencies.entry(tag.clone()).or_insert(0) += 1;

                // Count all ancestors
                for ancestor in tag_system.get_all_ancestors(tag) {
                    *frequencies.entry(ancestor).or_insert(0) += 1;
                }
            }
        }

        frequencies
    }

    /// Fetch all forms that have completed status and are eligible for matching
    pub(crate) async fn fetch_unmatched_forms(db_pool: &PgPool) -> Result<Vec<Form>, sqlx::Error> {
        sqlx::query_as!(
            Form,
            r#"
            SELECT user_id, gender as "gender: Gender", familiar_tags, aspirational_tags, recent_topics,
                   self_traits, ideal_traits, physical_boundary, self_intro, profile_photo_filename
            FROM forms f
            JOIN users u ON u.id = f.user_id
            WHERE u.status = 'form_completed'
            "#,
        )
        .fetch_all(db_pool)
        .await
    }

    /// Fetch all forms that have been submitted, regardless of user status
    /// Used for calculating tag frequencies to ensure stable IDF scores
    pub(crate) async fn fetch_all_submitted_forms(
        db_pool: &PgPool,
    ) -> Result<Vec<Form>, sqlx::Error> {
        sqlx::query_as!(
            Form,
            r#"
            SELECT user_id, gender as "gender: Gender", familiar_tags, aspirational_tags, recent_topics,
                   self_traits, ideal_traits, physical_boundary, self_intro, profile_photo_filename
            FROM forms
            "#,
        )
        .fetch_all(db_pool)
        .await
    }

    /// Store match preview in database using UPSERT operation
    async fn store_match_preview(
        db_pool: &PgPool,
        user_id: Uuid,
        candidate_ids: &[Uuid],
        scores: &[f64],
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            "INSERT INTO match_previews (user_id, candidate_ids, scores)
             VALUES ($1, $2, $3)
             ON CONFLICT (user_id)
             DO UPDATE SET
                candidate_ids = EXCLUDED.candidate_ids,
                scores = EXCLUDED.scores",
            user_id,
            candidate_ids,
            scores
        )
        .execute(db_pool)
        .await?;

        Ok(())
    }

    /// Build a map of vetoed_id -> set of vetoer_ids for efficient lookup
    pub(crate) async fn build_map_vetoed_as_key(
        db_pool: &PgPool,
    ) -> Result<HashMap<Uuid, HashSet<Uuid>>, sqlx::Error> {
        let vetoes = sqlx::query!("SELECT vetoer_id, vetoed_id FROM vetoes")
            .fetch_all(db_pool)
            .await?;

        let mut veto_map: HashMap<Uuid, HashSet<Uuid>> = HashMap::new();
        for veto in vetoes {
            veto_map
                .entry(veto.vetoed_id)
                .or_default()
                .insert(veto.vetoer_id);
        }

        Ok(veto_map)
    }

    /// Check if user_a has vetoed user_b
    pub(crate) fn is_vetoed(
        user_a: Uuid,
        user_b: Uuid,
        veto_map: &HashMap<Uuid, HashSet<Uuid>>,
    ) -> bool {
        veto_map
            .get(&user_a)
            .is_some_and(|vetoed_set| vetoed_set.contains(&user_b))
    }

    /// Spawn the periodic preview generation task
    pub fn spawn_preview_generation_task(db_pool: PgPool, tag_system: &'static TagSystem) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(MATCH_PREVIEW_INTERVAL);
            interval.tick().await; // First tick completes immediately, so we skip it

            loop {
                interval.tick().await;
                let _ = Self::generate_match_previews(&db_pool, tag_system).await;
            }
        });
    }
}
