use std::collections::HashMap;

use pathfinding::{kuhn_munkres::kuhn_munkres, matrix::Matrix};
use serde::Serialize;
use sqlx::PgPool;
use time::OffsetDateTime;
use tracing::{debug, error, info, instrument};
use uuid::Uuid;

use super::matching::MatchingService;
use crate::{
    error::{AppError, AppResult},
    models::{FinalMatch, Gender, ScheduleStatus, ScheduledFinalMatch, TagSystem},
    utils::{
        constant::{
            CHECK_AUTO_ACCEPT_INTERVAL, CHECK_SCHEDULED_MATCH_INTERVAL,
            FINAL_MATCH_AUTO_ACCEPT_TIMEOUT,
        },
        static_object::UPLOAD_DIR,
    },
};

#[derive(Debug, Serialize)]
struct DryRunMatch {
    user_a_id: Uuid,
    user_a_email: String,
    user_b_id: Uuid,
    user_b_email: String,
    score: f64,
}

#[derive(Debug, Serialize)]
struct DryRunOutput {
    timestamp: String,
    dry_run: bool,
    matches: Vec<DryRunMatch>,
}

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
    #[instrument(skip_all, err)]
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
            let matches_created =
                Self::execute_scheduled_final_match(db_pool, tag_system, due_match.id).await?;
            info!(
                scheduled_match_id = %due_match.id,
                %matches_created,
                "Scheduled final match completed successfully"
            );
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
        match Self::execute_final_matching(db_pool, tag_system, false).await {
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

    /// Execute the final matching algorithm using bipartite matching.
    ///
    /// Matches males to females using the Kuhn-Munkres algorithm.
    /// Users from the larger gender group may remain unmatched if sizes are unequal.
    ///
    /// If `dry_run` is true, simulates matching without database changes and saves
    /// results to a JSON file in UPLOAD_DIR.
    ///
    /// Ok value is the number of matches created
    pub async fn execute_final_matching(
        db_pool: &PgPool,
        tag_system: &TagSystem,
        dry_run: bool,
    ) -> AppResult<usize> {
        // Fetch unmatched users for matching
        let unmatched_forms = MatchingService::fetch_unmatched_forms(db_pool).await?;

        // Fetch ALL submitted forms for stable tag frequency calculation
        // This ensures IDF scores remain consistent across multiple matching runs
        let all_forms = MatchingService::fetch_all_submitted_forms(db_pool).await?;

        // Partition unmatched users by gender
        let mut males = Vec::new();
        let mut females = Vec::new();
        for form in &unmatched_forms {
            match form.gender {
                Gender::Male => males.push(form),
                Gender::Female => females.push(form),
            }
        }

        // Handle edge cases: need at least one of each gender
        if males.is_empty() || females.is_empty() {
            info!(
                males_count = males.len(),
                females_count = females.len(),
                "Cannot perform matching: need at least one user of each gender"
            );
            return Ok(0);
        }

        // Kuhn-Munkres requires rows <= columns, so use smaller set as rows
        let (rows, cols, transposed) = if males.len() <= females.len() {
            (males.as_slice(), females.as_slice(), false)
        } else {
            (females.as_slice(), males.as_slice(), true)
        };
        let rows_count = rows.len();
        let cols_count = cols.len();

        info!(
            males_count = males.len(),
            females_count = females.len(),
            rows_count,
            cols_count,
            transposed,
            "Starting bipartite matching"
        );

        // Fetch all veto records
        let veto_map = MatchingService::build_map_vetoed_as_key(db_pool).await?;

        // Calculate tag frequencies for IDF scoring using ALL forms (not just unmatched)
        let tag_frequencies = MatchingService::calculate_tag_frequencies(&all_forms, tag_system);
        let total_user_count = all_forms.len() as u32;

        // Build bipartite weight matrix
        // Requires Ord so we scale f64 scores by 1000 and convert to i64 to preserve precision
        // Usually scores are between 0.1 and 30.0, so this should be safe
        const SCALE_FACTOR: f64 = 1000.0;
        let mut weights = Matrix::new(rows_count, cols_count, 0_i64);
        let mut raw_scores = HashMap::new();

        for (i, form_row) in rows.iter().enumerate() {
            for (j, form_col) in cols.iter().enumerate() {
                let score = MatchingService::calculate_match_score(
                    form_row,
                    form_col,
                    tag_system,
                    &tag_frequencies,
                    total_user_count,
                );

                // Validate score is not NaN or infinite
                if !score.is_finite() {
                    error!(
                        user_row = %form_row.user_id,
                        user_col = %form_col.user_id,
                        score,
                        "Invalid score detected (NaN or infinity), skipping pair"
                    );
                    continue;
                }

                // Apply vetoes - if either user has vetoed the other, leave score at 0
                if MatchingService::is_vetoed(form_row.user_id, form_col.user_id, &veto_map)
                    || MatchingService::is_vetoed(form_col.user_id, form_row.user_id, &veto_map)
                {
                    continue;
                }

                // Only use positive scores
                if score > 0.0 {
                    // Scale and convert to integer for kuhn_munkres
                    let weight = (score * SCALE_FACTOR) as i64;
                    weights[(i, j)] = weight;

                    // Store raw score for final match creation
                    raw_scores.insert((i, j), score);
                }
            }
        }

        // Run Kuhn-Munkres algorithm to find maximum weight bipartite matching
        // Returns (total_weight, assignments) where assignments[i] = j means row[i] is matched to col[j]
        let (_, assignments) = kuhn_munkres(&weights);

        // Extract valid matches from assignments
        let mut matched_pairs = Vec::new();
        for (i, &j) in assignments.iter().enumerate() {
            // Skip zero-weight assignments (no compatible match found)
            if weights[(i, j)] <= 0 {
                continue;
            }

            let user_row = rows[i].user_id;
            let user_col = cols[j].user_id;

            // Get the original raw score for this match
            let score = raw_scores.get(&(i, j)).copied().ok_or_else(|| {
                error!(
                    %user_row, %user_col,
                    "Missing raw score for matched pair, this should not happen"
                );
                AppError::Internal
            })?;

            matched_pairs.push((user_row, user_col, score));
        }

        let matches_count = matched_pairs.len();

        if dry_run {
            // Dry run mode: save results to file without modifying database
            info!(
                matches_count,
                "Dry run mode: saving results to file without database changes"
            );

            // Fetch user emails for the matched pairs
            let mut dry_run_matches = Vec::new();
            for (user_a_id, user_b_id, score) in matched_pairs {
                let user_a = sqlx::query!(r#"SELECT email FROM users WHERE id = $1"#, user_a_id)
                    .fetch_one(db_pool)
                    .await?;

                let user_b = sqlx::query!(r#"SELECT email FROM users WHERE id = $1"#, user_b_id)
                    .fetch_one(db_pool)
                    .await?;

                dry_run_matches.push(DryRunMatch {
                    user_a_id,
                    user_a_email: user_a.email,
                    user_b_id,
                    user_b_email: user_b.email,
                    score,
                });
            }

            // Create output structure
            let output = DryRunOutput {
                timestamp: OffsetDateTime::now_utc().to_string(),
                dry_run: true,
                matches: dry_run_matches,
            };

            // Save to file
            let timestamp = OffsetDateTime::now_utc()
                .format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_else(|_| "unknown".to_string())
                .replace(':', "-"); // Replace colons for filesystem compatibility
            let filename = format!("dry_run_matches_{}.json", timestamp);
            let filepath = std::path::Path::new(UPLOAD_DIR.as_str()).join(&filename);

            // Ensure UPLOAD_DIR exists
            tokio::fs::create_dir_all(UPLOAD_DIR.as_str())
                .await
                .map_err(|e| {
                    error!("Failed to create UPLOAD_DIR: {}", e);
                    AppError::Internal
                })?;

            // Write JSON to file
            let json_content = serde_json::to_string_pretty(&output).map_err(|e| {
                error!("Failed to serialize dry run output: {}", e);
                AppError::Internal
            })?;

            tokio::fs::write(&filepath, json_content)
                .await
                .map_err(|e| {
                    error!("Failed to write dry run file: {}", e);
                    AppError::Internal
                })?;

            info!(
                filepath = %filepath.display(),
                "Dry run results saved to file"
            );
        } else {
            // Normal mode: persist matches to database
            let mut final_matches = Vec::new();
            for (user_row, user_col, score) in matched_pairs {
                // Create the final match
                let final_match =
                    Self::create_final_match(db_pool, user_row, user_col, score).await?;
                debug!(%final_match.id, %score, "Created a final pair");
                final_matches.push(final_match);
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
        }

        Ok(matches_count)
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

    /// Auto-accept final matches that have been pending for more than 24 hours
    #[instrument(skip_all, err)]
    pub async fn auto_accept_expired_matches(db_pool: &PgPool) -> AppResult<()> {
        let cutoff_time = OffsetDateTime::now_utc() - FINAL_MATCH_AUTO_ACCEPT_TIMEOUT;

        // Find final matches older than 24 hours where at least one user has not confirmed
        let expired_matches = sqlx::query!(
            r#"
            SELECT fm.id, fm.user_a_id, fm.user_b_id
            FROM final_matches fm
            JOIN users ua ON fm.user_a_id = ua.id
            JOIN users ub ON fm.user_b_id = ub.id
            WHERE fm.created_at <= $1
            AND (ua.status = 'matched' OR ub.status = 'matched')
            "#,
            cutoff_time
        )
        .fetch_all(db_pool)
        .await?;

        for expired_match in expired_matches {
            // Update both users to 'confirmed' status
            let mut tx = db_pool.begin().await?;

            let user_a_result = sqlx::query!(
                "UPDATE users SET status = 'confirmed' WHERE id = $1 AND status = 'matched'",
                expired_match.user_a_id
            )
            .execute(tx.as_mut())
            .await?;

            let user_b_result = sqlx::query!(
                "UPDATE users SET status = 'confirmed' WHERE id = $1 AND status = 'matched'",
                expired_match.user_b_id
            )
            .execute(tx.as_mut())
            .await?;

            // Only commit if at lease one user is still in 'matched' status
            if user_a_result.rows_affected() > 0 || user_b_result.rows_affected() > 0 {
                tx.commit().await?;
                info!(
                    final_match_id = %expired_match.id,
                    user_a_id = %expired_match.user_a_id,
                    user_b_id = %expired_match.user_b_id,
                    "Successfully auto-accepted expired final match"
                );
            } else {
                tx.rollback().await?;
                error!(final_match_id = %expired_match.id, "Data race detected while auto-accepting final match");
            }
        }

        Ok(())
    }

    /// Spawn the periodic task to auto-accept expired final matches
    pub fn spawn_auto_accept_task(db_pool: PgPool) {
        tokio::spawn(async move {
            // Check every 10 minutes for expired final matches
            let mut interval = tokio::time::interval(CHECK_AUTO_ACCEPT_INTERVAL);
            interval.tick().await; // First tick completes immediately, so we skip it

            loop {
                interval.tick().await;
                let _ = Self::auto_accept_expired_matches(&db_pool).await;
            }
        });
    }

    /// Spawn the periodic scheduler task to check for due scheduled matches
    pub fn spawn_scheduler_task(db_pool: PgPool, tag_system: &'static TagSystem) {
        tokio::spawn(async move {
            // Check every minute for due scheduled matches
            let mut interval = tokio::time::interval(CHECK_SCHEDULED_MATCH_INTERVAL);
            interval.tick().await; // First tick completes immediately, so we skip it

            loop {
                interval.tick().await;
                let _ = Self::check_and_execute_scheduled_matches(&db_pool, tag_system).await;
            }
        });
    }
}
