use async_trait::async_trait;
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EmailError {
    #[error("Failed to send email: {0}")]
    SendFailed(String),
}

#[async_trait]
pub trait EmailService: Send + Sync {
    /// Sends an email.
    ///
    /// This trait defines a method for sending emails. Implementations of this trait
    /// should provide the actual email sending logic.
    ///
    /// # Errors
    /// Returns an `EmailError` if the email sending fails.
    async fn send_email(
        &self,
        recipient: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<(), EmailError>;
}

pub struct LogEmailer;

#[async_trait]
impl EmailService for LogEmailer {
    async fn send_email(
        &self,
        recipient: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<(), EmailError> {
        println!("====== MOCK EMAIL SENT ======");
        println!("To: {recipient}");
        println!("Subject: {subject}");
        println!("-----------------------------");
        println!("{body_html}");
        println!("=============================");

        Ok(())
    }
}

pub struct MailgunEmailer {
    api_key: String,
    sender_email: String,
    http_client: reqwest::Client,
}

impl MailgunEmailer {
    pub fn new(api_key: String, sender_email: String) -> Self {
        Self {
            api_key,
            sender_email,
            http_client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl EmailService for MailgunEmailer {
    async fn send_email(
        &self,
        recipient: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<(), EmailError> {
        let api_url = "https://api.sendgrid.com/v3/mail/send";

        let payload = json!({
            "personalizations": [{ "to": [{ "email": recipient }] }],
            "from": { "email": &self.sender_email },
            "subject": subject,
            "content": [{ "type": "text/html", "value": body_html }]
        });

        let response = self
            .http_client
            .post(api_url)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await;

        match response {
            Ok(res) if res.status().is_success() => Ok(()),
            Ok(res) => {
                let error_body = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Failed to read error response body".to_string());
                Err(EmailError::SendFailed(format!(
                    "Mailgun API error: {error_body}"
                )))
            }
            Err(e) => Err(EmailError::SendFailed(format!(
                "Network request error: {e}"
            ))),
        }
    }
}
