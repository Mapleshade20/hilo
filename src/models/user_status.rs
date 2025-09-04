//! # User Status Types
//!
//! This module defines the UserStatus enum that corresponds to the PostgreSQL
//! user_status enum type in the database. Using a Rust enum provides better
//! performance compared to text conversion and ensures type safety.

use axum::response::IntoResponse;
use serde::{Deserialize, Serialize};

/// Represents the possible status values for a user in the system.
///
/// This enum corresponds directly to the PostgreSQL `user_status` enum type
/// defined in the database migrations. Using this enum instead of text
/// conversion provides better performance and compile-time type safety.
///
/// # Status Flow
///
/// The typical user progression through statuses:
/// - `Unverified` - Email verified, but card not verified
/// - `VerificationPending` - Email verified, card photo uploaded, awaiting admin verification
/// - `Verified` - Email and card verified, but form not completed
/// - `FormCompleted` - Form completed, waiting to be matched
/// - `Matched` - Matched pair generated, awaiting confirmation from both parties
/// - `Confirmed` - Match confirmed
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "user_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum UserStatus {
    /// Email verified, but card not verified
    Unverified,
    /// Email verified, card photo uploaded, awaiting admin verification
    VerificationPending,
    /// Email and card verified, but form not completed
    Verified,
    /// Form completed, waiting to be matched
    FormCompleted,
    /// Matched pair generated, awaiting confirmation from both parties
    Matched,
    /// Match confirmed
    Confirmed,
}

impl std::fmt::Display for UserStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status_str = match self {
            UserStatus::Unverified => "unverified",
            UserStatus::VerificationPending => "verification_pending",
            UserStatus::Verified => "verified",
            UserStatus::FormCompleted => "form_completed",
            UserStatus::Matched => "matched",
            UserStatus::Confirmed => "confirmed",
        };
        write!(f, "{status_str}")
    }
}

impl UserStatus {
    /// Returns true if the user is allowed to upload a card photo.
    /// Only unverified users can upload cards.
    #[inline]
    pub fn can_upload_card(&self) -> bool {
        matches!(self, UserStatus::Unverified)
    }

    /// Returns true if the user is allowed to fill/update a form or upload a profile photo.
    /// Only verified and form_completed users have access.
    #[inline]
    pub fn can_fill_form(&self) -> bool {
        matches!(self, UserStatus::Verified | UserStatus::FormCompleted)
    }

    /// Returns true if the user has completed card verification.
    #[inline]
    pub fn is_card_verified(&self) -> bool {
        matches!(
            self,
            UserStatus::Verified
                | UserStatus::FormCompleted
                | UserStatus::Matched
                | UserStatus::Confirmed
        )
    }

    /// Queries the database for the user's status by their user ID.
    pub async fn query(
        db_pool: &sqlx::PgPool,
        user_id: &uuid::Uuid,
    ) -> Result<Self, impl IntoResponse> {
        use axum::http::StatusCode;
        let user_status_result = sqlx::query!(
            r#"SELECT status as "status: UserStatus" FROM users WHERE id = $1"#,
            user_id
        )
        .fetch_optional(db_pool)
        .await;

        match user_status_result {
            Ok(Some(row)) => Ok(row.status),
            Ok(None) => Err((StatusCode::NOT_FOUND, "User not found")),
            Err(_) => Err((StatusCode::INTERNAL_SERVER_ERROR, "Database error")),
        }
    }
}
