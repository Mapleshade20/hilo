//! # Email Service
//!
//! This module provides email sending functionality with multiple implementations.
//! The service trait allows for easy testing and switching between different
//! email providers or mock implementations.
//!
//! ## Implementations
//!
//! - [`LogEmailer`] - Development/testing implementation that logs emails to console
//! - [`ExternalEmailer`] - Production implementation using external email API

use async_trait::async_trait;
use serde_json::json;
use thiserror::Error;
use tracing::{debug, error, info, instrument};

use crate::utils::html::generate_verification_email_html;

/// Errors that can occur during email operations
#[derive(Debug, Error)]
pub enum EmailError {
    #[error("Failed to send email: {0}")]
    SendFailed(String),
}

/// Trait for email sending services
///
/// This trait provides a common interface for different email implementations,
/// allowing the application to switch between real email providers and mock
/// implementations for testing.
#[async_trait]
pub trait EmailService: Send + Sync {
    /// Sends an email to the specified recipient.
    ///
    /// # Arguments
    ///
    /// * `recipient` - Email address of the recipient
    /// * `subject` - Email subject line
    /// * `core` - The most essential field; structs implementing this trait should format it
    ///
    /// # Errors
    ///
    /// Returns [`EmailError::SendFailed`] if the email cannot be sent due to
    /// network issues, API errors, or other delivery problems.
    async fn send_email(
        &self,
        recipient: &str,
        subject: &str,
        core: &str,
    ) -> Result<(), EmailError>;
}

/// Mock email service for development and testing
///
/// This implementation logs email details to the console instead of sending
/// real emails. Useful for development environments and automated testing
/// where actual email delivery is not desired.
pub struct LogEmailer;

#[async_trait]
impl EmailService for LogEmailer {
    #[instrument(skip(self, code))]
    async fn send_email(
        &self,
        recipient: &str,
        subject: &str,
        code: &str,
    ) -> Result<(), EmailError> {
        info!("Sending mock email");

        println!("====== MOCK EMAIL SENT ======");
        println!("To: {recipient}");
        println!("Subject: {subject}");
        println!("-----------------------------");
        println!("Your verification code is: {code}");
        println!("=============================");

        debug!("Mock email logged to console");
        Ok(())
    }
}

/// External email service for production use
///
/// This implementation sends emails through an external email API provider.
/// It requires API credentials and handles HTTP communication with the email service.
///
/// # Configuration
///
/// Requires the following environment variables in production:
/// - `MAIL_API_URL` - Base URL of the email API
/// - `MAIL_API_KEY` - Authentication key for the email API
/// - `SENDER_EMAIL` - Email address to use as sender
pub struct ExternalEmailer {
    api_url: String,
    api_key: String,
    sender_email: String,
    http_client: reqwest::Client,
}

impl ExternalEmailer {
    /// Creates a new external email service instance.
    ///
    /// # Arguments
    ///
    /// * `api_url` - Base URL of the email API endpoint
    /// * `api_key` - Authentication key for the email service
    /// * `sender_email` - Email address to use as the sender
    pub fn new(api_url: String, api_key: String, sender_email: String) -> Self {
        info!(
            api_url = %api_url,
            sender_email = %sender_email,
            "Initializing external email service"
        );

        Self {
            api_url,
            api_key,
            sender_email,
            http_client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl EmailService for ExternalEmailer {
    #[instrument(
        skip(self, code),
        fields(
            sender = %self.sender_email
        )
    )]
    async fn send_email(
        &self,
        recipient: &str,
        subject: &str,
        code: &str,
    ) -> Result<(), EmailError> {
        debug!("Preparing to send email via external API");

        let payload = json!({
            "to": recipient,
            "from": self.sender_email,
            "subject": subject,
            "content": [{ "type": "text/html", "value": generate_verification_email_html(code) }]
        });

        debug!("Sending HTTP request to email API");
        let response = self
            .http_client
            .post(&self.api_url)
            .basic_auth("api", Some(&self.api_key))
            .json(&payload)
            .send()
            .await;

        match response {
            Ok(res) if res.status().is_success() => {
                info!("Email sent successfully via external API");
                Ok(())
            }
            Ok(res) => {
                let status = res.status();
                let error_body = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Failed to read error response body".to_string());

                error!(
                    status = %status,
                    error_body = %error_body,
                    "External email API returned error"
                );

                Err(EmailError::SendFailed(format!(
                    "Third party email provider API error: {error_body}"
                )))
            }
            Err(e) => {
                error!(error = %e, "Network request to email API failed");
                Err(EmailError::SendFailed(format!(
                    "Network request error: {e}"
                )))
            }
        }
    }
}
