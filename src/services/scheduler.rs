use std::collections::HashSet;

use sqlx::PgPool;
use time::OffsetDateTime;
use tracing::{debug, error, info, instrument};
use uuid::Uuid;

use super::matching::MatchingService;
use crate::error::{AppError, AppResult};
use crate::models::{FinalMatch, ScheduleStatus, ScheduledFinalMatch, TagSystem};
use crate::utils::constant::CHECK_SCHEDULED_MATCH_INTERVAL;

pub struct SchedulerService;

impl SchedulerService {
    /// Get the next scheduled final match time (earliest pending match)
    pub async fn get_next_scheduled_time(db_pool: &PgPool) -> AppResult<Option<OffsetDateTime>> {
        let result = sqlx::query!(
            r#"
            SELECT scheduled_time
            FROM scheduled_final_matches
            WHERE status = 'pending'
            ORDER BY scheduled_time ASC
            LIMIT 1
            "#
        )
        .fetch_optional(db_pool)
        .await?;

        Ok(result.map(|row| row.scheduled_time))
    }

    /// Create multiple scheduled final match triggers
    pub async fn create_scheduled_matches(
        db_pool: &PgPool,
        scheduled_times: &[OffsetDateTime],
    ) -> AppResult<Vec<ScheduledFinalMatch>> {
        let mut scheduled_matches = Vec::new();

        for &scheduled_time in scheduled_times {
            // Validate that the time is in the future
            if scheduled_time <= OffsetDateTime::now_utc() {
                return Err(AppError::BadRequest("Scheduled time must be in the future"));
            }

            let scheduled_match = sqlx::query_as!(
                ScheduledFinalMatch,
                r#"
                INSERT INTO scheduled_final_matches (scheduled_time)
                VALUES ($1)
                ON CONFLICT (scheduled_time)
                DO UPDATE SET scheduled_time = EXCLUDED.scheduled_time
                RETURNING id, scheduled_time, status as "status: ScheduleStatus",
                         created_at, executed_at, matches_created, error_message
                "#,
                scheduled_time
            )
            .fetch_one(db_pool)
            .await?;

            scheduled_matches.push(scheduled_match);
        }

        Ok(scheduled_matches)
    }

    /// Get all scheduled matches (admin view)
    pub async fn get_all_scheduled_matches(
        db_pool: &PgPool,
    ) -> AppResult<Vec<ScheduledFinalMatch>> {
        let scheduled_matches = sqlx::query_as!(
            ScheduledFinalMatch,
            r#"
            SELECT id, scheduled_time, status as "status: ScheduleStatus",
                   created_at, executed_at, matches_created, error_message
            FROM scheduled_final_matches
            ORDER BY scheduled_time ASC
            "#
        )
        .fetch_all(db_pool)
        .await?;

        Ok(scheduled_matches)
    }

    /// Cancel a scheduled match (delete it)
    pub async fn cancel_scheduled_match(db_pool: &PgPool, match_id: Uuid) -> AppResult<bool> {
        let result = sqlx::query!(
            r#"
            DELETE FROM scheduled_final_matches
            WHERE id = $1 AND status = 'pending'
            "#,
            match_id
        )
        .execute(db_pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    /// Check for and execute any due scheduled matches
    async fn check_and_execute_scheduled_matches(
        db_pool: &PgPool,
        tag_system: &TagSystem,
    ) -> AppResult<()> {
        // Find all pending matches that are due
        let due_matches = sqlx::query_as!(
            ScheduledFinalMatch,
            r#"
            SELECT id, scheduled_time, status as "status: ScheduleStatus",
                   created_at, executed_at, matches_created, error_message
            FROM scheduled_final_matches
            WHERE status = 'pending' AND scheduled_time <= CURRENT_TIMESTAMP
            ORDER BY scheduled_time ASC
            "#
        )
        .fetch_all(db_pool)
        .await?;

        for due_match in due_matches {
            debug!(
                scheduled_match_id = %due_match.id,
                scheduled_time = %due_match.scheduled_time,
                "Executing scheduled final match"
            );

            // Execute the final matching
            match Self::execute_scheduled_final_match(db_pool, tag_system, due_match.id).await {
                Ok(matches_created) => {
                    info!(
                        scheduled_match_id = %due_match.id,
                        matches_created,
                        "Scheduled final match completed successfully"
                    );
                }
                Err(e) => {
                    error!(
                        scheduled_match_id = %due_match.id,
                        error = %e,
                        "Scheduled final match failed"
                    );
                }
            }
        }

        Ok(())
    }

    /// Execute a specific scheduled final match, returning number of matches created
    async fn execute_scheduled_final_match(
        db_pool: &PgPool,
        tag_system: &TagSystem,
        scheduled_match_id: Uuid,
    ) -> AppResult<usize> {
        let now = OffsetDateTime::now_utc();

        // Update status to indicate execution is starting
        sqlx::query!(
            r#"
            UPDATE scheduled_final_matches
            SET executed_at = $1
            WHERE id = $2 AND status = 'pending'
            "#,
            now,
            scheduled_match_id
        )
        .execute(db_pool)
        .await?;

        // Execute the final matching algorithm
        match Self::execute_final_matching(db_pool, tag_system).await {
            Ok(matches_created) => {
                // Update status to completed
                sqlx::query!(
                    r#"
                    UPDATE scheduled_final_matches
                    SET status = 'completed', matches_created = $1
                    WHERE id = $2
                    "#,
                    matches_created as i32,
                    scheduled_match_id
                )
                .execute(db_pool)
                .await?;

                Ok(matches_created)
            }
            Err(e) => {
                // Update status to failed with error message
                let error_message = e.to_string();
                sqlx::query!(
                    r#"
                    UPDATE scheduled_final_matches
                    SET status = 'failed', error_message = $1
                    WHERE id = $2
                    "#,
                    error_message,
                    scheduled_match_id
                )
                .execute(db_pool)
                .await?;

                Err(e)
            }
        }
    }

    /// Execute the final matching algorithm using greedy approach
    ///
    /// Ok value is the number of matches created
    pub async fn execute_final_matching(
        db_pool: &PgPool,
        tag_system: &TagSystem,
    ) -> AppResult<usize> {
        // Fetch all users with submitted forms
        let forms = MatchingService::fetch_unmatched_forms(db_pool).await?;
        if forms.is_empty() {
            return Ok(0);
        }

        // Fetch all veto records
        let veto_map = MatchingService::build_map_vetoed_as_key(db_pool).await?;

        // Calculate tag frequencies for IDF scoring
        let tag_frequencies = MatchingService::calculate_tag_frequencies(&forms);
        let total_user_count = forms.len() as u32;

        // Build score matrix for all valid pairs
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
                if MatchingService::is_vetoed(form_a.user_id, form_b.user_id, &veto_map)
                    || MatchingService::is_vetoed(form_b.user_id, form_a.user_id, &veto_map)
                {
                    continue; // Skip vetoed pairs entirely
                }

                if score > 0.0 {
                    pair_scores.push((form_a.user_id, form_b.user_id, score));
                }
            }
        }

        // Sort by score (descending) for greedy algorithm
        // TODO: consider using better algorithm like Hungarian for optimal matching
        pair_scores.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Greedy matching algorithm
        let mut matched_users = HashSet::new();
        let mut final_matches = Vec::new();

        for (user_a, user_b, score) in pair_scores {
            if !matched_users.contains(&user_a) && !matched_users.contains(&user_b) {
                // Create the final match
                let final_match = Self::create_final_match(db_pool, user_a, user_b, score).await?;
                final_matches.push(final_match);

                matched_users.insert(user_a);
                matched_users.insert(user_b);
            }
        }

        // Update status of matched users to 'matched'
        for final_match in &final_matches {
            sqlx::query!(
                r#"UPDATE users SET status = 'matched' WHERE id = $1"#,
                final_match.user_a_id
            )
            .execute(db_pool)
            .await?;

            sqlx::query!(
                r#"UPDATE users SET status = 'matched' WHERE id = $1"#,
                final_match.user_b_id
            )
            .execute(db_pool)
            .await?;
        }

        // Clear all vetoes and previews after final matching
        info!("Clearing all vetoes and match previews");
        sqlx::query!("DELETE FROM vetoes").execute(db_pool).await?;
        sqlx::query!("DELETE FROM match_previews")
            .execute(db_pool)
            .await?;

        Ok(final_matches.len())
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

    /// Spawn the periodic scheduler task to check for due scheduled matches
    #[instrument(skip_all)]
    pub fn spawn_scheduler_task(db_pool: PgPool, tag_system: &'static TagSystem) {
        tokio::spawn(async move {
            // Check every minute for due scheduled matches
            let mut interval = tokio::time::interval(CHECK_SCHEDULED_MATCH_INTERVAL);
            interval.tick().await; // First tick completes immediately, so we skip it

            loop {
                interval.tick().await;

                if let Err(e) =
                    Self::check_and_execute_scheduled_matches(&db_pool, tag_system).await
                {
                    error!("Failed to check and execute scheduled matches: {}", e);
                }
            }
        });
    }
}
