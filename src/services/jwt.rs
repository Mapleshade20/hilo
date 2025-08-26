use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use thiserror::Error;
use uuid::Uuid;

use crate::utils::constant::{ACCESS_TOKEN_EXPIRY, REFRESH_TOKEN_EXPIRY};

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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String, // user_id as string
    pub exp: u64,    // expiration timestamp
    pub iat: u64,    // issued at timestamp
}

#[derive(Debug, Serialize)]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_in: u64, // access token expiry in seconds
}

pub struct JwtService {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtService {
    pub fn new(encoding_key: EncodingKey, decoding_key: DecodingKey) -> Self {
        Self {
            encoding_key,
            decoding_key,
        }
    }

    pub async fn create_token_pair(
        &self,
        user_id: Uuid,
        db_pool: &PgPool,
    ) -> Result<TokenPair, JwtError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let access_token_exp = now + ACCESS_TOKEN_EXPIRY.as_secs();
        let refresh_token_exp = now + REFRESH_TOKEN_EXPIRY.as_secs();

        // Create access token
        let access_claims = Claims {
            sub: user_id.to_string(),
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

    pub fn validate_access_token(&self, token: &str) -> Result<Claims, JwtError> {
        match decode::<Claims>(token, &self.decoding_key, &Validation::default()) {
            Ok(token_data) => Ok(token_data.claims),
            Err(e) if e.kind() == &jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                Err(JwtError::TokenExpired)
            }
            Err(_) => Err(JwtError::InvalidToken),
        }
    }

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
