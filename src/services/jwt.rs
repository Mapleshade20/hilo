//! # JWT Service
//!
//! This module provides JSON Web Token (JWT) functionality for user authentication.
//! It handles token creation, validation, and refresh token management with
//! secure database storage.
//!
//! ## Features
//!
//! - Access token generation and validation
//! - Refresh token management with database persistence
//! - Token rotation for enhanced security
//! - Bulk token revocation for user sessions
//!
//! ## Security
//!
//! - Refresh tokens are hashed before database storage
//! - Tokens have configurable expiration times
//! - Old refresh tokens are invalidated when new ones are issued (token rotation)

use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::utils::constant::*;

/// Errors that can occur during JWT operations
#[derive(Debug, Error)]
pub enum JwtError {
    #[error("Token encoding failed: {0}")]
    EncodingError(#[from] jsonwebtoken::errors::Error),
    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),
    #[error("Invalid token")]
    InvalidToken,
    #[error("Token expired")]
    TokenExpired,
    #[error("Refresh token not found")]
    RefreshTokenNotFound,
}

/// JWT claims structure for access tokens
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Subject (user ID as string)
    pub sub: String,
    /// Expiration timestamp (Unix epoch)
    pub exp: u64,
    /// Issued at timestamp (Unix epoch)
    pub iat: u64,
}

/// Token pair containing access and refresh tokens
#[derive(Debug, Serialize)]
pub struct TokenPair {
    /// JWT access token for API authentication
    pub access_token: String,
    /// Refresh token for obtaining new access tokens
    pub refresh_token: String,
    /// Access token expiry time in seconds
    pub expires_in: u64,
}

/// Service for managing JWT tokens and refresh token lifecycle
pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtService {
    /// Creates a new JWT service with the provided keys.
    ///
    /// # Arguments
    ///
    /// * `encoding_key` - Key used for signing JWT tokens
    /// * `decoding_key` - Key used for verifying JWT tokens
    pub fn new(encoding_key: EncodingKey, decoding_key: DecodingKey) -> Self {
        Self {
            encoding_key,
            decoding_key,
        }
    }

    /// Creates a new access and refresh token pair for the user.
    ///
    /// The refresh token is securely hashed before storage in the database.
    /// Access tokens are short-lived while refresh tokens have longer expiration.
    ///
    /// # Arguments
    ///
    /// * `user_id` - Unique identifier for the user
    /// * `db_pool` - Database connection pool for storing refresh token
    ///
    /// # Returns
    ///
    /// Returns a [`TokenPair`] containing both tokens and expiration info.
    ///
    /// # Errors
    ///
    /// Returns [`JwtError`] if token creation or database storage fails.
    pub async fn create_token_pair(
        &self,
        user_id: Uuid,
        db_pool: &PgPool,
    ) -> Result<TokenPair, JwtError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("System time should not be before UNIX EPOCH")
            .as_secs();

        let access_token_exp = now + ACCESS_TOKEN_EXPIRY.as_secs();
        let refresh_token_exp = now + REFRESH_TOKEN_EXPIRY.as_secs();

        // Create access token
        let access_claims = Claims {
            sub: user_id.as_simple().to_string(),
            exp: access_token_exp,
            iat: now,
        };
        let access_token = encode(&Header::default(), &access_claims, &self.encoding_key)?;

        // Create refresh token
        let refresh_token = Uuid::new_v4().to_string();
        let mut hasher = Sha256::new();
        hasher.update(refresh_token.as_bytes());
        let refresh_token_hash = format!("{:x}", hasher.finalize());

        // Store refresh token in database
        sqlx::query!(
            r#"
            INSERT INTO refresh_tokens (user_id, token_hash, expires_at) 
            VALUES ($1, $2, to_timestamp($3))
            "#,
            user_id,
            refresh_token_hash,
            refresh_token_exp as i64
        )
        .execute(db_pool)
        .await?;

        Ok(TokenPair {
            access_token,
            refresh_token,
            expires_in: ACCESS_TOKEN_EXPIRY.as_secs(),
        })
    }

    /// Validates an access token and returns its claims.
    ///
    /// This method verifies the token signature and checks expiration.
    /// It does not perform database lookups for validation.
    ///
    /// # Arguments
    ///
    /// * `token` - JWT access token to validate
    ///
    /// # Returns
    ///
    /// Returns the [`Claims`] if the token is valid and not expired.
    ///
    /// # Errors
    ///
    /// - [`JwtError::TokenExpired`] - Token has expired
    /// - [`JwtError::InvalidToken`] - Token is malformed or has invalid signature
    pub fn validate_access_token(&self, token: &str) -> Result<Claims, JwtError> {
        match decode::<Claims>(token, &self.decoding_key, &Validation::default()) {
            Ok(token_data) => Ok(token_data.claims),
            Err(e) if e.kind() == &jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                Err(JwtError::TokenExpired)
            }
            Err(_) => Err(JwtError::InvalidToken),
        }
    }

    /// Creates a new token pair using a valid refresh token.
    ///
    /// This method implements token rotation - the old refresh token is invalidated
    /// and a new refresh token is created along with a new access token.
    ///
    /// # Arguments
    ///
    /// * `refresh_token` - Current valid refresh token
    /// * `db_pool` - Database connection pool for token validation and storage
    ///
    /// # Returns
    ///
    /// Returns a new [`TokenPair`] with fresh tokens.
    ///
    /// # Errors
    ///
    /// - [`JwtError::RefreshTokenNotFound`] - Token not found or expired
    /// - [`JwtError::DatabaseError`] - Database operation failed
    pub async fn refresh_token_pair(
        &self,
        refresh_token: &str,
        db_pool: &PgPool,
    ) -> Result<TokenPair, JwtError> {
        let mut hasher = Sha256::new();
        hasher.update(refresh_token.as_bytes());
        let refresh_token_hash = format!("{:x}", hasher.finalize());

        // Verify refresh token exists and is not expired
        let token_record = sqlx::query!(
            r#"
            SELECT user_id, expires_at
            FROM refresh_tokens 
            WHERE token_hash = $1 AND expires_at > NOW()
            "#,
            refresh_token_hash
        )
        .fetch_optional(db_pool)
        .await?;

        let token_record = token_record.ok_or(JwtError::RefreshTokenNotFound)?;

        // Delete the old refresh token (token rotation)
        sqlx::query!(
            "DELETE FROM refresh_tokens WHERE token_hash = $1",
            refresh_token_hash
        )
        .execute(db_pool)
        .await?;

        // Create new token pair
        self.create_token_pair(token_record.user_id, db_pool).await
    }

    /// Revokes a specific refresh token.
    ///
    /// This method removes the refresh token from the database, preventing
    /// its future use. Useful for implementing logout functionality.
    ///
    /// # Arguments
    ///
    /// * `refresh_token` - Refresh token to revoke
    /// * `db_pool` - Database connection pool
    ///
    /// # Errors
    ///
    /// Returns [`JwtError::DatabaseError`] if the database operation fails.
    pub async fn revoke_refresh_token(
        &self,
        refresh_token: &str,
        db_pool: &PgPool,
    ) -> Result<(), JwtError> {
        let mut hasher = Sha256::new();
        hasher.update(refresh_token.as_bytes());
        let refresh_token_hash = format!("{:x}", hasher.finalize());

        sqlx::query!(
            "DELETE FROM refresh_tokens WHERE token_hash = $1",
            refresh_token_hash
        )
        .execute(db_pool)
        .await?;

        Ok(())
    }

    /// Revokes all refresh tokens for a specific user.
    ///
    /// This method removes all refresh tokens associated with a user,
    /// effectively logging them out from all devices. Useful for security
    /// purposes or account management.
    ///
    /// # Arguments
    ///
    /// * `user_id` - User whose tokens should be revoked
    /// * `db_pool` - Database connection pool
    ///
    /// # Errors
    ///
    /// Returns [`JwtError::DatabaseError`] if the database operation fails.
    pub async fn revoke_user_refresh_token(
        &self,
        user_id: Uuid,
        db_pool: &PgPool,
    ) -> Result<(), JwtError> {
        sqlx::query!("DELETE FROM refresh_tokens WHERE user_id = $1", user_id)
            .execute(db_pool)
            .await?;

        Ok(())
    }
}
