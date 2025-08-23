#![allow(dead_code)]

use async_trait::async_trait;
use hilo::services::email::{EmailError, EmailService};
use std::sync::{Arc, Mutex};
use tokio::net::TcpListener;

/// A mock email service that stores sent emails for testing purposes.
/// This is ideal for integration tests as it doesn't produce console output.
#[derive(Debug, Default)]
pub struct MockEmailer {
    sent_emails: Mutex<Vec<SentEmail>>,
}

#[derive(Debug, Clone)]
pub struct SentEmail {
    pub recipient: String,
    pub subject: String,
    pub body_html: String,
}

impl MockEmailer {
    pub fn new() -> Self {
        Self {
            sent_emails: Mutex::new(Vec::new()),
        }
    }

    /// Get all sent emails for testing verification
    pub fn get_sent_emails(&self) -> Vec<SentEmail> {
        self.sent_emails.lock().unwrap().clone()
    }

    /// Clear all stored emails
    pub fn clear(&self) {
        self.sent_emails.lock().unwrap().clear();
    }

    /// Get the count of sent emails
    pub fn sent_count(&self) -> usize {
        self.sent_emails.lock().unwrap().len()
    }

    /// Get the last sent email
    pub fn last_sent_email(&self) -> Option<SentEmail> {
        self.sent_emails.lock().unwrap().last().cloned()
    }
}

#[async_trait]
impl EmailService for MockEmailer {
    async fn send_email(
        &self,
        recipient: &str,
        subject: &str,
        body_html: &str,
    ) -> Result<(), EmailError> {
        let email = SentEmail {
            recipient: recipient.to_string(),
            subject: subject.to_string(),
            body_html: body_html.to_string(),
        };

        self.sent_emails.lock().unwrap().push(email);
        Ok(())
    }
}

/// Spawns the application and returns its address and mock emailer for testing.
pub async fn spawn_app() -> (String, Arc<MockEmailer>) {
    dotenvy::dotenv().ok();

    let mock_emailer = Arc::new(MockEmailer::new());
    let mock_cloned = mock_emailer.clone();

    // Randomly choose an available port
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Failed to bind random port at localhost");

    let port = listener.local_addr().unwrap().port();

    tokio::spawn(async move {
        let app = hilo::app_with_email_service(Some(mock_cloned));
        axum::serve(listener, app).await.unwrap();
    });

    let address = format!("http://127.0.0.1:{}", port);
    (address, mock_emailer)
}
